/*
 * mtasim_c.c - Multi-tape cellular automata simulator (C port of the Rust backend).
 *
 * This is the compiled, runnable artifact. It reads a transition table (JSON)
 * and an initial tape state (JSON), runs N steps, and reports the verdict.
 *
 * The engine is a faithful port of src/automata/multi_tape_automata.rs and
 * multi_tape_product_trie.rs from the Rust crate.
 *
 * Build:  gcc -O2 -o mtasim mtasim_c.c -lm
 * Run:    ./mtasim trans.json tapes.json <num_steps> <verdict_tape> <verdict_pos>
 *
 * JSON transition table format:
 *   [ { "input_terms": [[tape,pos,state], ...], "out_tape": T, "out_state": S }, ... ]
 *
 * JSON initial tapes format:
 *   { "num_tapes": N, "tapes": [ {pos: state, ...}, ... ] }
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

typedef struct { int tape; int pos; int state; } Term;
typedef struct { Term *terms; int nterms; int out_tape; int out_state; char ann[128]; } Transition;

static Transition *transitions = NULL;
static int ntransitions = 0;

/* --- Minimal JSON parsing (no external deps) --- */

/* Skip whitespace. */
static const char *skip_ws(const char *p) { while (*p==' '||*p=='\t'||*p=='\n'||*p=='\r') p++; return p; }

/* Parse an integer. Returns pointer past it. */
static const char *parse_int(const char *p, long *out) {
    p = skip_ws(p);
    int neg = 0; if (*p=='-'){neg=1;p++;}
    long v=0;
    while (*p>='0'&&*p<='9'){ v = v*10 + (*p-'0'); p++; }
    *out = neg ? -v : v;
    return p;
}

/* Parse a JSON array of arrays of 3 ints: [[t,p,s],...] into a list of Terms. */
static const char *parse_term_array(const char *p, Term **out_terms, int *out_n) {
    p = skip_ws(p);
    if (*p!='[') { fprintf(stderr,"expected [ at %s\n", p); exit(1); }
    p++;
    Term *terms = NULL; int n = 0, cap = 0;
    for(;;) {
        p = skip_ws(p);
        if (*p==']') { p++; break; }
        if (*p==',') { p++; continue; }
        /* parse [t,p,s] */
        if (*p!='[') { fprintf(stderr,"expected [ term at %s\n",p); exit(1);}
        p++;
        long t,v,s;
        p = parse_int(p,&t); p = skip_ws(p); if(*p==',')p++;
        p = parse_int(p,&v); p = skip_ws(p); if(*p==',')p++;
        p = parse_int(p,&s); p = skip_ws(p); if(*p==']')p++;
        if (n==cap){ cap = cap?cap*2:16; terms = realloc(terms, cap*sizeof(Term)); }
        terms[n].tape=(int)t; terms[n].pos=(int)v; terms[n].state=(int)s; n++;
    }
    *out_terms = terms; *out_n = n;
    return p;
}

/* Parse the transition table JSON. */
static void parse_transitions(const char *json) {
    const char *p = skip_ws(json);
    if (*p!='[') { fprintf(stderr,"transitions: expected [ \n"); exit(1); }
    p++;
    int cap = 64;
    transitions = malloc(cap*sizeof(Transition));
    for(;;) {
        p = skip_ws(p);
        if (*p==']') { p++; break; }
        if (*p==',') { p++; continue; }
        if (ntransitions==cap){ cap*=2; transitions=realloc(transitions, cap*sizeof(Transition)); }
        Transition *tr = &transitions[ntransitions];
        memset(tr,0,sizeof(Transition));
        if (*p!='{') { fprintf(stderr,"expected { \n"); exit(1);}
        p++;
        for(;;) {
            p = skip_ws(p);
            if (*p=='}') { p++; break; }
            if (*p==',') { p++; continue; }
            /* parse key */
            if (*p!='"') { fprintf(stderr,"expected key\n"); exit(1);}
            p++;
            const char *key_start = p;
            while (*p!='"') p++;
            int keylen = (int)(p - key_start);
            char key[64]; if (keylen>63) keylen=63;
            memcpy(key, key_start, keylen); key[keylen]=0;
            p++;
            p = skip_ws(p); if (*p==':') p++;
            if (strcmp(key,"input_terms")==0) {
                p = parse_term_array(p, &tr->terms, &tr->nterms);
            } else if (strcmp(key,"out_tape")==0) {
                long ot; p = parse_int(p,&ot); tr->out_tape=(int)ot;
            } else if (strcmp(key,"out_state")==0) {
                long os; p = parse_int(p,&os); tr->out_state=(int)os;
            } else if (strcmp(key,"annotation")==0) { p=skip_ws(p);
                if (*p!='"'){fprintf(stderr,"expected string\n");exit(1);}
                p++;
                int i=0; while(*p!='"' && *p){ tr->ann[i++]=*p; p++; if(i>=127)break; }
                tr->ann[i]=0; p++;
            } else {
                /* skip unknown value: object or array or scalar */
                if (*p=='{') { int d=1; p++; while(d){ if(*p=='{')d++; else if(*p=='}')d--; else if(*p=='"'){p++; while(*p!='"')p++;} p++; if(!*p)break; } }
                else if (*p=='[') { int d=1; p++; while(d){ if(*p=='[')d++; else if(*p==']')d--; else if(*p=='"'){p++; while(*p!='"')p++;} p++; if(!*p)break; } }
                else if (*p=='"') { p++; while(*p!='"')p++; p++; }
                else { while(*p && *p!=',' && *p!='}' && *p!=']') p++; }
            }
        }
        ntransitions++;
    }
}

/* --- Tape storage: sparse map keyed by (tape,pos) --- */

typedef struct { int tape, pos, state; int next; } CellEntry;
static CellEntry *cells = NULL;
static int ncells = 0, cellcap = 0;
static int *tape_hash = NULL; /* hash table per (tape,pos) -> index into cells */
static size_t HT_SIZE = 1<<16;

static size_t cellhash(int tape, int pos) {
    uint64_t h = (uint64_t)((int32_t)tape) * 1000003u + (uint64_t)((uint32_t)pos);
    h ^= h >> 16; h *= 0x45d9f3b; h ^= h >> 16;
    return (size_t)(h & (HT_SIZE-1));
}

static void ensure_cellcap(void) {
    if (ncells >= cellcap) {
        cellcap = cellcap ? cellcap*2 : 1024;
        cells = realloc(cells, cellcap*sizeof(CellEntry));
    }
}

static int get_cell(int tape, int pos) {
    size_t i = cellhash(tape,pos);
    while (tape_hash[i] >= 0) {
        int idx = tape_hash[i];
        if (cells[idx].tape==tape && cells[idx].pos==pos) return cells[idx].state;
        i = (i+1) & (HT_SIZE-1);
    }
    return 0;
}

static void set_cell_direct(int tape, int pos, int state) {
    if (state == 0) {
        /* remove if present */
        size_t i = cellhash(tape,pos);
        while (tape_hash[i] >= 0) {
            int idx = tape_hash[i];
            if (cells[idx].tape==tape && cells[idx].pos==pos) {
                int last = tape_hash[i];
                tape_hash[i] = cells[last].next;
                cells[last].next = -1;
                return;
            }
            i = (i+1) & (HT_SIZE-1);
        }
        return;
    }
    ensure_cellcap();
    size_t i = cellhash(tape,pos);
    while (tape_hash[i] >= 0) {
        int idx = tape_hash[i];
        if (cells[idx].tape==tape && cells[idx].pos==pos) { cells[idx].state=state; return; }
        i = (i+1) & (HT_SIZE-1);
    }
    int idx = ncells++;
    cells[idx].tape=tape; cells[idx].pos=pos; cells[idx].state=state; cells[idx].next=tape_hash[i];
    tape_hash[i]=idx;
}

/* --- Product trie step (port of process_step) --- */

typedef struct { int tape, pos; int state; int present; } Write;
static Write *writes = NULL;
static int nwrites = 0, writecap = 0;

static void record_write(int tape, int pos, int state) {
    /* check for conflict */
    for (int i=0;i<nwrites;i++) {
        if (writes[i].tape==tape && writes[i].pos==pos) {
            if (writes[i].present && writes[i].state != state) {
                fprintf(stderr,"CONFLICT at tape=%d pos=%d: state %d vs %d\n", tape,pos,writes[i].state,state);
                exit(2);
            }
            return; /* same state, ignore */
        }
    }
    if (nwrites==writecap){ writecap = writecap?writecap*2:256; writes=realloc(writes,writecap*sizeof(Write)); }
    writes[nwrites].tape=tape; writes[nwrites].pos=pos; writes[nwrites].state=state; writes[nwrites].present=1;
    nwrites++;
}

static int range_min = 0, range_max = 0;
static void update_range(int pos) {
    if (pos < range_min) range_min = pos;
    if (pos > range_max) range_max = pos;
}

static void process_step(void) {
    nwrites = 0;
    int lo = range_min - 2;
    int hi = range_max + 2;
    for (int p = lo; p <= hi; p++) {
        for (int ti = 0; ti < ntransitions; ti++) {
            Transition *tr = &transitions[ti];
            int match = 1;
            for (int k = 0; k < tr->nterms; k++) {
                Term *tm = &tr->terms[k];
                int actual = get_cell(tm->tape, p + tm->pos);
                if (actual != tm->state) { match = 0; break; }
            }
            if (match) {
                record_write(tr->out_tape, p, tr->out_state);
                update_range(p);
            }
        }
    }
    for (int i = 0; i < nwrites; i++)
        set_cell_direct(writes[i].tape, writes[i].pos, writes[i].state);
}

/* --- Initial tapes JSON --- */
static void parse_tapes(const char *json, int *num_tapes_out) {
    const char *p = json;
    /* find "num_tapes": N */
    const char *nt = strstr(json, "\"num_tapes\"");
    if (nt) {
        const char *c = strchr(nt+11, ':');
        long v; parse_int(c+1, &v); *num_tapes_out = (int)v;
    } else *num_tapes_out = 5;
    /* For each "tapes" array element (an object of pos:state), set cells.
       Simple approach: find each tape object and parse "pos":state pairs. */
    const char *tp = strstr(json, "\"tapes\"");
    if (!tp) return;
    p = strchr(tp, '['); if(!p) return; p++;
    int tape_no = 0;
    for(;;) {
        p = skip_ws(p);
        if (*p==']') break;
        if (*p==','){p++;continue;}
        if (*p!='{'){fprintf(stderr,"tapes: expected {\n");exit(1);}
        p++;
        for(;;) {
            p = skip_ws(p);
            if (*p=='}'){p++;break;}
            if (*p==','){p++;continue;}
            if (*p!='"'){fprintf(stderr,"tapes: expected key\n");exit(1);}
            p++;
            const char *ks=p; while(*p!='"')p++;
            int kl=(int)(p-ks); long pos; char buf[32]; if(kl>31)kl=31; memcpy(buf,ks,kl);buf[kl]=0; pos=atoll(buf);
            p++;
            p=skip_ws(p); if(*p==':')p++;
            long st; p=parse_int(p,&st);
            set_cell_direct(tape_no,(int)pos,(int)st);
            update_range((int)pos);
        }
        tape_no++;
    }
}

static char *read_file(const char *path) {
    FILE *f = fopen(path,"rb"); if(!f){perror(path);exit(1);}
    fseek(f,0,SEEK_END); long sz=ftell(f); fseek(f,0,SEEK_SET);
    char *buf = malloc(sz+1); fread(buf,1,sz,f); buf[sz]=0; fclose(f);
    return buf;
}

int main(int argc, char **argv) {
    if (argc < 4) {
        fprintf(stderr,"Usage: %s trans.json tapes.json <num_steps> [verdict_tape] [verdict_pos]\n", argv[0]);
        return 1;
    }
    char *tjson = read_file(argv[1]);
    char *ijson = read_file(argv[2]);
    int steps = atoi(argv[3]);
    int vtape = argc>4 ? atoi(argv[4]) : 4;
    int vpos = argc>5 ? atoi(argv[5]) : -1;

    parse_transitions(tjson);
    int num_tapes = 0;
    tape_hash = malloc(HT_SIZE*sizeof(int));
    memset(tape_hash, 0xFF, HT_SIZE*sizeof(int)); /* -1 sentinel */
    parse_tapes(ijson, &num_tapes);

    printf("Loaded %d transitions, %d non-empty initial cells\n", ntransitions, ncells);
    if (getenv("DBG")) {
        printf("DBG cell(0,8)=%d cell(1,8)=%d cell(2,8)=%d cell(3,8)=%d cell(1,9)=%d\n",
            get_cell(0,8),get_cell(1,8),get_cell(2,8),get_cell(3,8),get_cell(1,9));
        printf("DBG range=[%d,%d]\n", range_min, range_max);
        // Find rule SCAN_START_2_2_0 and test match at MID=8
        for (int ti=0; ti<ntransitions; ti++){
            if (strcmp(transitions[ti].ann,"SCAN_START_2_2_0")==0){
                printf("DBG found rule at idx %d, nterms=%d\n", ti, transitions[ti].nterms);
                int match=1;
                for (int k=0;k<transitions[ti].nterms;k++){
                    Term *tm=&transitions[ti].terms[k];
                    int actual=get_cell(tm->tape, 8+tm->pos);
                    printf("DBG  term tape=%d pos=%d want=%d got=%d %s\n",
                        tm->tape, tm->pos, tm->state, actual, actual==tm->state?"OK":"FAIL");
                    if(actual!=tm->state) match=0;
                }
                printf("DBG  overall match: %s\n", match?"YES":"NO");
            }
        }
    }
    int trace = getenv("TRACE")!=NULL;
    for (int s = 0; s < steps; s++) {
        process_step();
        if (trace) {
            printf("step %d: scan=", s+1);
            for (int p = range_min; p<=range_max; p++){ int v=get_cell(3,p); if(v) printf("%d:%d ", p,v);}
            printf("verdict(t4,p-1)=%d\n", get_cell(4,-1));
        }
    }
    /* Report verdict */
    int verdict = get_cell(vtape, vpos);
    printf("verdict tape=%d pos=%d state=%d\n", vtape, vpos, verdict);
    if (verdict==4) printf("RESULT: SAT\n");
    else if (verdict==3) printf("RESULT: UNSAT\n");
    else printf("RESULT: PENDING\n");
    return 0;
}
/* (debug helpers appended for tracing) */

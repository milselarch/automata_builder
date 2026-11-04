import time
from automata_builder.compiled_counter_automata import (
    CompiledCounterAutomataRunner, CounterAutomataSettings
)
from automata_builder.rule_generator import BLANK_INT

t0 = time.time()
runner = CompiledCounterAutomataRunner(
    initial_write_start=0,
    initial_write_end=20,
    verbose=False,
    automata_settings=CounterAutomataSettings(
        base=2, apply_reduction=True, collapse_reduction_tape=False,
    )
)
t1 = time.time()
print(f'PHASE build: {t1 - t0:.2f}s')
print(f'num transitions = {len(runner.transitions_group.transitions)}')

t2 = time.time()
runner.run_simulation(
    num_timesteps=30,
    terminal_width=BLANK_INT,
    render_start=-5,
)
t3 = time.time()
print(f'PHASE simulate: {t3 - t2:.2f}s')
print(f'TOTAL: {t3 - t0:.2f}s')

from automata_builder._rust import D

a = D(0, 0, 1) | D(1, 1, 2) * D(2, 0, 3)
print(a)
print(a * a)
print(a | a)

import unittest

from automata_builder.counter_states import build_st_counter_state
from automata_builder.counter_automata import (
    active_counter,
    from_counter_state,
    paused_counter,
)


class TestCounterStateEncoding(unittest.TestCase):
    def test_build_and_from_are_inverse(self) -> None:
        # Treat build_st_counter_state as "to_counter_state".
        for digit in range(0, 128):
            for paused in (False, True):
                encoded = build_st_counter_state(digit, paused)
                decoded_digit, decoded_paused = from_counter_state(encoded)

                self.assertEqual(decoded_digit, digit)
                self.assertEqual(decoded_paused, paused)

                # Round-trip back to the same encoded value.
                re_encoded = build_st_counter_state(
                    decoded_digit, decoded_paused
                )
                self.assertEqual(re_encoded, encoded)

    def test_helper_encoders_match_build(self) -> None:
        for digit in range(0, 128):
            self.assertEqual(
                active_counter(digit),
                build_st_counter_state(digit, paused=False),
            )
            self.assertEqual(
                paused_counter(digit),
                build_st_counter_state(digit, paused=True),
            )


if __name__ == "__main__":
    unittest.main()

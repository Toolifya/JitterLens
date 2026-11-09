import os

def test_random_like():
    # Deterministic "random-like" pattern based on iteration for reliable validation.
    # (A true random test would make the test suite non-deterministic.)
    i = int(os.environ.get("JITTERLENS_ITERATION", "1"))
    # Pattern: pass, fail, fail, pass, fail, pass, fail, fail, pass, pass...
    assert i % 3 != 0, f"intentional failure on iteration {i}"

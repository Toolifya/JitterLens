import os

def test_intermittent():
    # Fails on even iterations, passes on odd iterations (deterministic).
    i = int(os.environ.get("JITTERLENS_ITERATION", "1"))
    assert i % 2 == 1, f"intentional failure on iteration {i}"

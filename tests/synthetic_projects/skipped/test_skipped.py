import pytest

@pytest.mark.skip(reason="always skipped for fixture")
def test_always_skipped():
    assert True

import os

def pytest_collection_modifyitems(config, items):
    i = int(os.environ.get("JITTERLENS_ITERATION", "1"))
    if i == 2:
        # Make test_disappears vanish from this iteration's results.
        keep = [it for it in items if it.name != "test_disappears"]
        items[:] = keep

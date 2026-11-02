"""Explicitly mark tests that require isolated trainer/benchmark processes.

Ordinary unittest discovery still runs everything. The shared runner can select
these tests with --suite integration or leave them out with --suite unit.
"""


def integration(test):
    test.test_suite = "integration"
    return test

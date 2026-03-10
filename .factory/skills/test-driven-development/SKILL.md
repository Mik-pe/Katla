# Test-Driven Development

Practices for using TDD to find bugs and iterate on implementations.

## Bugs

For bug solving we SHOULD:
- Write FAILING test(s) for the correct behaviour we WANT happen.
  - The tests should aim to mimic the behaviour of the buggy codepath.
  - The codepath may need to first be rewritten in a way that provides easier testability, such as providing good structures and functions to make it easier to setup or mock.
- Check that the test(s) FAIL since the bug disrupts the behaviour
- Find and implement a solution to the problem
- Verify that the test(s) PASS and the bug is solved.

Bug fixing should try to:
- Reuse existing code, while fixing bad code paths
- Extend existing code, to favor composition
- If possible, solve more than this bug, providing a better overall design

## Iterate on Feature

When iterating on a feature TDD should:
- Write FAILING test(s) which may or may not compile, providing a base of the behaviours our feature wants to solve
- Bit-by-bit implement the feature such that all tests PASS

Implementing features should try to:
- Reuse existing code, while fixing bad code paths
- Extend existing code, to favor composition
- Fit the overall design of our codebase.

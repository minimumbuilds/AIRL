Write a function called `safe-index` that takes a list, an index, and a default value, returning the element at the index or the default if out of bounds.

Requirements:
- The :requires contract must enforce that the index is non-negative
- The :ensures contract must guarantee the result is valid
- If the index is within bounds, return the element
- If the index is out of bounds (>= length), return the default
- The function must NOT crash on out-of-bounds access

Test case: safe-index([10, 20, 30], 1, -1) should return 20
Test case: safe-index([10, 20, 30], 5, -1) should return -1

Print the result of calling the function with arguments [100, 200, 300], 2, and 0.

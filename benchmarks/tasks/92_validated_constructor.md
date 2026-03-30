Write a function called `make-range` that creates a two-element list [lo, hi] representing a numeric range, with validation.

Requirements:
- The :requires contract must enforce lo <= hi
- The :ensures contract must enforce that the result has exactly 2 elements
- Return a list [lo, hi]
- Also write a function `in-range` that checks if a value is within a range

Test case: make-range(1, 10) should return [1, 10]
Test case: in-range(5, [1, 10]) should return true
Test case: in-range(15, [1, 10]) should return false

Print the result of calling in-range with 7 and the range created by make-range(1, 10).

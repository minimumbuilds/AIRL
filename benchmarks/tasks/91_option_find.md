Write a function called `find-first-even` that takes a list of integers and returns the first even number, or nil if none exists.

Requirements:
- Return the value directly if found (not wrapped in a variant)
- Return nil if no even number exists
- Check evenness using modulo 2
- The caller should check for nil before using the result

Test case: find-first-even([1, 3, 4, 7]) should return 4
Test case: find-first-even([1, 3, 5]) should return nil

Call the function with [7, 3, 8, 1, 6]. Print "Found: N" if an even number is found, or "None" if nil is returned.

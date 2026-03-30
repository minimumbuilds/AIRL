Write a function called `clamp-list` that takes a list of integers, a minimum value, and a maximum value, and returns a new list with each element clamped to the range [min, max].

Requirements:
- Values below min are replaced with min
- Values above max are replaced with max
- Values within range are unchanged
- The result list has the same length as the input

Test case: clamp-list([1, 5, 10, 15, 20], 5, 15) should return [5, 5, 10, 15, 15]
Test case: clamp-list([], 0, 100) should return []

Print the result of calling the function with arguments [-3, 0, 5, 10, 15], 0, and 10.

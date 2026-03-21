Write a function called `positive-square-sum` that takes a list of integers and performs a multi-step pipeline: filter to keep only positive numbers, square each one, then sum the results.

Requirements:
- Negative numbers and zero are excluded
- Each positive number is squared before summing
- An empty list or all-negative list returns 0
- The result must be >= 0

Test case: positive-square-sum([1, -2, 3, -4, 5]) should return 35 (1 + 9 + 25)
Test case: positive-square-sum([-1, -2]) should return 0

Print the result of calling the function with argument [3, -1, 4, -1, 5, -9, 2].

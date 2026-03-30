Write a function called `dedup-stable` that removes duplicates from a list while preserving the order of first occurrence.

Requirements:
- Keep the first occurrence of each element
- Remove all subsequent occurrences
- The result list should have no duplicate elements
- An empty list returns an empty list

Test case: dedup-stable([3, 1, 4, 1, 5, 9, 2, 6, 5, 3]) should return [3, 1, 4, 5, 9, 2, 6]
Test case: dedup-stable([1, 1, 1]) should return [1]

Print the result of calling the function with argument [5, 3, 5, 1, 3, 2, 1, 4].

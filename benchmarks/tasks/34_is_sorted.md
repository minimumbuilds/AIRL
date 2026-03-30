Write a function called `is-sorted` that checks whether a list of integers is sorted in non-decreasing order.

Requirements:
- Return true if each element is less than or equal to the next
- An empty list is considered sorted
- A single-element list is considered sorted
- Duplicate adjacent values are allowed (non-decreasing, not strictly increasing)

Test case: is-sorted([1, 2, 3, 4, 5]) should return true
Test case: is-sorted([1, 3, 2, 4]) should return false
Test case: is-sorted([1, 1, 2, 2]) should return true

Print the result of calling the function with argument [1, 2, 2, 3, 5].

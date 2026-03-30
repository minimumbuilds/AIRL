Write a function called `merge-sort` that sorts a list of integers in ascending order using the merge sort algorithm.

Requirements:
- Implement recursive merge sort (split, sort halves, merge)
- Use take and drop to split the list in half
- Merge two sorted halves by comparing heads
- An empty or single-element list is already sorted

Test case: merge-sort([5, 3, 8, 1, 9, 2]) should return [1, 2, 3, 5, 8, 9]
Test case: merge-sort([]) should return []

Print the result of calling the function with argument [38, 27, 43, 3, 9, 82, 10].

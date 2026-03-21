Write a function called `group-by-parity` that takes a list of integers and separates them into even and odd numbers, returning both lists.

Requirements:
- Return a pair of lists: (evens, odds)
- Preserve the relative order within each group
- Every element from the input must appear in exactly one of the two output lists

Test case: group-by-parity([1, 2, 3, 4, 5, 6]) should return ([2, 4, 6], [1, 3, 5])
Test case: group-by-parity([]) should return ([], [])

Print the result of calling the function with argument [10, 15, 20, 25, 30].

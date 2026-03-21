Write a function called `flatten` that takes a list of lists of integers and returns a single flat list containing all elements.

Requirements:
- Concatenate all inner lists in order
- Empty inner lists contribute no elements
- An empty outer list returns an empty list

Test case: flatten([[1, 2], [3], [4, 5, 6]]) should return [1, 2, 3, 4, 5, 6]
Test case: flatten([[], [1], []]) should return [1]

Print the result of calling the function with argument [[1, 2], [3, 4], [5]].

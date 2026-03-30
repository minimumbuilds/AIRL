Write a function called `chunk` that splits a list into groups of N elements.

Requirements:
- Each chunk has exactly N elements, except possibly the last which may be shorter
- Chunks preserve element order
- An empty list returns an empty list
- N must be positive

Test case: chunk([1, 2, 3, 4, 5], 2) should return [[1, 2], [3, 4], [5]]
Test case: chunk([1, 2, 3, 4], 4) should return [[1, 2, 3, 4]]

Print the result of calling the function with arguments [1, 2, 3, 4, 5, 6, 7] and 3.

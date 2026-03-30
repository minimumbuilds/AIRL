Write a function called `take-while-positive` that takes a list of integers and returns elements from the beginning as long as they are positive.

Requirements:
- Stop at the first non-positive element (zero or negative)
- Return the prefix of positive elements
- An empty list returns an empty list
- If all elements are positive, return the entire list

Test case: take-while-positive([3, 5, 2, -1, 4]) should return [3, 5, 2]
Test case: take-while-positive([-1, 2, 3]) should return []

Print the result of calling the function with argument [7, 3, 9, 0, 5, 2].

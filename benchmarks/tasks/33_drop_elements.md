Write a function called `drop-while-even` that takes a list of integers and drops elements from the beginning as long as they are even, returning the rest.

Requirements:
- Skip leading even elements
- Return everything starting from the first odd element
- An empty list returns an empty list
- If all elements are even, return an empty list

Test case: drop-while-even([2, 4, 6, 3, 8, 10]) should return [3, 8, 10]
Test case: drop-while-even([1, 2, 3]) should return [1, 2, 3]
Test case: drop-while-even([2, 4, 6]) should return []

Print the result of calling the function with argument [4, 8, 2, 7, 6].

Write a function called `cumulative-product` that takes a list of integers and returns a list where each element is the product of all elements up to and including that index.

Requirements:
- The i-th element of the result is the product of elements 0 through i
- The result has the same length as the input
- An empty list returns an empty list

Test case: cumulative-product([1, 2, 3, 4]) should return [1, 2, 6, 24]
Test case: cumulative-product([2, 3, 5]) should return [2, 6, 30]

Print the result of calling the function with argument [1, 2, 3, 4, 5].

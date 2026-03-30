Write a function called `running-average` that takes a list of integers and returns a list of running average values as integers (truncated, not rounded).

Requirements:
- The i-th element of the result is the integer average of elements 0 through i
- Use integer division (truncation toward zero)
- The result has the same length as the input
- An empty list returns an empty list

Test case: running-average([10, 20, 30]) should return [10, 15, 20]
Test case: running-average([3, 1, 4]) should return [3, 2, 2]

Print the result of calling the function with argument [10, 20, 30, 40, 50].

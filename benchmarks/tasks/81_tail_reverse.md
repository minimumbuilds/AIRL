Write a function called `reverse-acc` that reverses a list using an accumulator pattern (tail-recursive style).

Requirements:
- Use a helper function with an accumulator that builds the reversed list
- Do not use the built-in `reverse` function
- Move elements one at a time from the front of the input to the front of the accumulator
- An empty list returns an empty list

Test case: reverse-acc([1, 2, 3, 4]) should return [4, 3, 2, 1]
Test case: reverse-acc([]) should return []

Print the result of calling the function with argument [10, 20, 30, 40, 50].

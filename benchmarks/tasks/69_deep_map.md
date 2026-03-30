Write a function called `double-all` that takes a nested list structure (lists within lists) and doubles every integer found at any depth.

Requirements:
- Recursively traverse nested lists
- Double each integer value
- Preserve the nesting structure
- An empty list returns an empty list

Test case: double-all([1, [2, 3], [[4]]]) should return [2, [4, 6], [[8]]]
Test case: double-all([10, 20]) should return [20, 40]

Print the result of calling the function with argument [1, [2, [3, 4], 5], 6].

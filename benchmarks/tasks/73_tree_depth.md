Write a function called `max-depth` that computes the maximum nesting depth of a list structure.

Requirements:
- A flat list (no nested lists) has depth 1
- Each level of nesting adds 1 to the depth
- An empty list has depth 1
- Use recursion to traverse nested lists

Test case: max-depth([1, 2, 3]) should return 1
Test case: max-depth([1, [2, 3], 4]) should return 2
Test case: max-depth([1, [2, [3]]]) should return 3

Print the result of calling the function with argument [1, [2, [3, [4]]], [5, 6]].

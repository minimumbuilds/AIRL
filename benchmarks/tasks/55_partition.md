Write a function called `partition` that takes a predicate function and a list, returning two lists: elements that satisfy the predicate and elements that don't.

Requirements:
- Return a two-element list: [matching, non-matching]
- Preserve the relative order in each group
- An empty list returns [[], []]

Test case: partition with (> x 3) on [1, 5, 2, 4, 3, 6] should return [[5, 4, 6], [1, 2, 3]]

Print the result of calling the function with a predicate that checks if a number is even, applied to the list [1, 2, 3, 4, 5, 6, 7, 8].

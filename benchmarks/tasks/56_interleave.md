Write a function called `interleave` that takes two lists and returns a new list with elements alternating from each.

Requirements:
- Take one element from the first list, then one from the second, alternating
- If one list is longer, append its remaining elements at the end
- An empty list interleaved with a non-empty list returns the non-empty list

Test case: interleave([1, 3, 5], [2, 4, 6]) should return [1, 2, 3, 4, 5, 6]
Test case: interleave([1, 2], [10, 20, 30, 40]) should return [1, 10, 2, 20, 30, 40]

Print the result of calling the function with arguments [1, 3, 5, 7] and [2, 4].

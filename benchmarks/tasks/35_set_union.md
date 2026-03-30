Write a function called `list-union` that takes two lists and returns a list containing all unique elements from both.

Requirements:
- The result should contain no duplicates
- Preserve the order of first occurrence (elements from list1 first, then new elements from list2)
- An empty list unioned with any list returns that list (deduplicated)

Test case: list-union([1, 2, 3], [2, 3, 4, 5]) should return [1, 2, 3, 4, 5]
Test case: list-union([1, 1, 2], [2, 3]) should return [1, 2, 3]

Print the result of calling the function with arguments [1, 3, 5] and [2, 3, 4, 5, 6].

Write a function called `deep-flatten` that flattens an arbitrarily nested list structure into a single flat list.

Requirements:
- Recursively flatten all levels of nesting
- Preserve the left-to-right order of elements
- Non-list elements are kept as-is
- An empty list returns an empty list

Test case: deep-flatten([1, [2, [3, 4], 5], [6]]) should return [1, 2, 3, 4, 5, 6]
Test case: deep-flatten([[[[1]]]]) should return [1]

Print the result of calling the function with argument [1, [2, 3], [4, [5, [6, 7]]]].

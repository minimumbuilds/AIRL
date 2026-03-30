Write a function called `rle-decode` that decodes a run-length encoded list.

Requirements:
- Input is a list of [value, count] pairs
- Expand each pair into count copies of the value
- Return the flat decoded list
- An empty input returns an empty list

Test case: rle-decode([[1, 3], [2, 2], [3, 1]]) should return [1, 1, 1, 2, 2, 3]
Test case: rle-decode([["a", 2], ["b", 3]]) should return ["a", "a", "b", "b", "b"]

Print the result of calling the function with argument [[1, 2], [2, 1], [3, 3], [1, 1]].

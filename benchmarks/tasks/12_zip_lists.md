Write a function called `zip-lists` that takes two lists and returns a list of pairs, pairing elements at matching indices.

Requirements:
- The result length must equal the minimum of the two input lengths
- Each element in the result is a pair of corresponding elements
- Extra elements from the longer list are discarded

Test case: zip-lists([1, 2, 3], [10, 20, 30]) should return [(1, 10), (2, 20), (3, 30)]
Test case: zip-lists([1, 2], [10, 20, 30]) should return [(1, 10), (2, 20)]

Print the result of calling the function with arguments [1, 2, 3] and [10, 20, 30].

Write a function called `top-words` that takes a string and an integer N, and returns the N most frequent words as [word, count] pairs, sorted by count descending.

Requirements:
- Words are separated by spaces
- If two words have the same count, preserve first-occurrence order
- If N is greater than the number of unique words, return all of them
- Return a list of [word, count] pairs

Test case: top-words("a b a c b a", 2) should return [["a", 3], ["b", 2]]

Print the result of calling the function with arguments "the cat sat on the mat the cat" and 3.

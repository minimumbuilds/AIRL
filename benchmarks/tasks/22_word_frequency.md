Write a function called `word-frequency` that takes a string and returns a list of [word, count] pairs representing how many times each word appears.

Requirements:
- Words are separated by spaces
- Return pairs in order of first occurrence
- Each word should appear exactly once in the result

Print the result of calling the function with argument "one two one three two one".

---TESTS---
word-frequency("the cat and the dog") => [["the", 2], ["cat", 1], ["and", 1], ["dog", 1]]
word-frequency("one two one three two one") => [["one", 3], ["two", 2], ["three", 1]]
word-frequency("hello") => [["hello", 1]]
word-frequency("a a a") => [["a", 3]]

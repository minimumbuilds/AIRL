Write a function called `censor-word` that takes a string and a word to censor, replacing all occurrences of that word with asterisks of the same length.

Requirements:
- Replace every occurrence of the target word with asterisks
- The number of asterisks should equal the length of the censored word
- Matching is case-sensitive
- If the word is not found, return the original string unchanged

Test case: censor-word("the cat sat on the mat", "the") should return "*** cat sat on *** mat"
Test case: censor-word("hello", "xyz") should return "hello"

Print the result of calling the function with arguments "foo bar foo baz foo" and "foo".

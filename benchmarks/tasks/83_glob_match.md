Write a function called `glob-match` that checks if a string matches a simple glob pattern with '*' wildcards.

Requirements:
- '*' matches zero or more of any character
- All other characters match literally
- The entire string must match the entire pattern
- Case-sensitive matching

Test case: glob-match("hello", "h*o") should return true
Test case: glob-match("hello", "h*x") should return false
Test case: glob-match("", "*") should return true
Test case: glob-match("abc", "abc") should return true

Print the result of calling the function with arguments "hello world" and "hello*".

Write a function called `count-chars` that takes a string and returns a map of character frequencies.

Requirements:
- Each key is a single-character string
- Each value is the count of that character's occurrences
- Use map-get-or with default 0 to handle first occurrences
- Skip spaces
- Case-sensitive (treat 'A' and 'a' as different)

Test case: count-chars("aab") should return {"a": 2, "b": 1}
Test case: count-chars("") should return an empty map

Print the result of calling the function with argument "hello".

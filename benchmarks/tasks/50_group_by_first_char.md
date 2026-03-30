Write a function called `group-by-initial` that takes a list of strings and returns a map where each key is the first character and the value is a list of strings starting with that character.

Requirements:
- Group strings by their first character
- Each key in the result map is a single-character string
- Values are lists of strings that start with that character, in original order
- Empty strings should be skipped

Test case: group-by-initial(["apple", "ant", "banana", "avocado"]) should return {"a": ["apple", "ant", "avocado"], "b": ["banana"]}

Print the result of calling the function with argument ["red", "blue", "green", "ruby", "gold", "brown"].

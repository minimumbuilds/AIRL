Write a function called `build-trie` that takes a list of words and builds a trie (prefix tree) represented as nested maps.

Requirements:
- Each node is a map where keys are single characters and values are child maps
- Mark end-of-word with a special key "$" mapped to true
- Return the root map of the trie
- An empty word list returns an empty map

Test case: build-trie(["hi"]) should return {"h": {"i": {"$": true}}}
Test case: build-trie(["hi", "he"]) should return {"h": {"i": {"$": true}, "e": {"$": true}}}

Print the result of calling the function with argument ["cat", "car", "do"].

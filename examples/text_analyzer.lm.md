# Text Analyzer

Text analysis tool demonstrating string operations, maps, and data processing.

This example shows how to build practical utilities in Lumen using built-in data structures
and string manipulation capabilities.

```lumen
cell count_words(text: string) -> int
    # Count words by counting spaces + 1 (simplified)
    let count = 1  # At least one word if non-empty
    let i = 0
    while i < len(text)
        let char = text[i]
        if char == " "
            let count = count + 1
        end
        let i = i + 1
    end
    if len(text) == 0
        0
    else
        count
    end
end

cell contains_char(text: string, target: string) -> bool
    let i = 0
    while i < len(text)
        let char = text[i]
        if char == target
            return true
        end
        let i = i + 1
    end
    false
end

cell count_character(text: string, target: string) -> int
    let count = 0
    let i = 0
    while i < len(text)
        let char = text[i]
        if char == target
            let count = count + 1
        end
        let i = i + 1
    end
    count
end

cell analyze_text(text: string)
    print("=== Text Analysis ===")
    print("")
    print("Input text:")
    print("  \"{text}\"")
    print("")

    let word_count = count_words(text)
    let char_count = len(text)

    print("Statistics:")
    print("  Total words: {word_count}")
    print("  Character count: {char_count}")

    if word_count > 0
        let avg_length = char_count / word_count
        print("  Average word length: ~{avg_length} chars")
    end

    print("")
    print("Character Frequencies:")
    let space_count = count_character(text, " ")
    let e_count = count_character(text, "e")
    let t_count = count_character(text, "t")
    let a_count = count_character(text, "a")
    let o_count = count_character(text, "o")

    print("  ' ' (space): {space_count}")
    print("  'e': {e_count}")
    print("  't': {t_count}")
    print("  'a': {a_count}")
    print("  'o': {o_count}")
end

cell main()
    let sample_text = "The quick brown fox jumps over the lazy dog"

    analyze_text(sample_text)

    print("")
    print("=== Another Example ===")
    print("")

    let text2 = "Lumen is a language for systems"
    analyze_text(text2)
end
```

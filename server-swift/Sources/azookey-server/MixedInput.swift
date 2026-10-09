import Foundation
import KanaKanjiConverterModule

// The word data is SCOWL, not Meltype program code. See Resources/SCOWL-LICENSE.txt.
enum MixedInput {
    struct Node: Sendable {
        var children: [Character: Int] = [:]
        var word: String?
        var product = false
    }
    static let dictionary: Result<[Node], Error> = Result {
        let url = Bundle.module.bundleURL.appendingPathComponent("english-words.txt")
        let words = parseWords(try String(contentsOf: url, encoding: .utf8))
        var nodes = [Node()]
        for word in words + products.sorted() {
            var node = 0
            for character in word {
                if let child = nodes[node].children[character] {
                    node = child
                } else {
                    let child = nodes.count
                    nodes[node].children[character] = child
                    nodes.append(Node())
                    node = child
                }
            }
            nodes[node].word = word
            nodes[node].product = products.contains(word)
        }
        return nodes
    }
    /// Accept both LF and Windows CRLF without adding newline characters to dictionary words.
    static func parseWords(_ text: String) -> [String] {
        text.split(whereSeparator: \.isNewline)
            .filter { !$0.hasPrefix("#") && $0.count >= 3 }.map(String.init)
    }
    // Product names absent from a general spelling dictionary. Written independently.
    static let products: Set<String> = [
        "adobe", "amazon", "android", "azookey", "chatgpt", "chrome", "docker",
        "firefox", "github", "gitlab", "google", "javascript", "linux", "microsoft",
        "netflix", "openai", "slack", "spotify", "typescript", "ubuntu", "vscode",
        "youtube",
    ]

    static func romanized(_ text: String, style: InputStyle) -> ComposingText {
        var result = ComposingText()
        result.insertAtCursorPosition(text, inputStyle: style)
        return result
    }

    static func elements(_ raw: String, style: InputStyle) -> [ComposingText.InputElement] {
        guard case .success(let nodes) = dictionary else {
            preconditionFailure("English dictionary must be loaded before editing")
        }
        let characters = Array(raw)
        let lowercase = characters.map { $0.isASCII ? Character(String($0).lowercased()) : $0 }
        // Keep the explicit Shift-started Latin input behavior, including unknown names.
        if characters.first.map({ $0.isASCII && $0.isUppercase }) == true {
            return characters.map { .init(character: $0, inputStyle: .direct) }
        }
        // Custom tables and AZIK must retain their own interpretation of Latin strokes.
        guard style == .roman2kana else {
            return characters.map { .init(character: $0, inputStyle: style) }
        }
        var result: [ComposingText.InputElement] = []
        var start = 0
        var kanaStart = 0
        while start < characters.count {
            var end = start
            var node = 0
            var matches: [(Int, Int)] = []
            while end < min(characters.count, start + 32), characters[end].isASCII, characters[end].isLetter,
                  let child = nodes[node].children[lowercase[end]] {
                node = child
                end += 1
                if nodes[node].word != nil { matches.append((end, node)) }
            }
            var englishEnd: Int?
            for (candidateEnd, node) in matches.reversed() {
                let word = nodes[node].word!
                // Short English spellings (ash, mas, etc.) often occur inside
                // Japanese romaji. Recognize three letters only at a word end.
                if candidateEnd - start == 3, candidateEnd < characters.count,
                   characters[candidateEnd].isASCII && characters[candidateEnd].isLetter { continue }
                let reading = romanized(word, style: style).convertTarget
                // A spelling such as "make" is also ordinary Japanese romaji.
                // Preserve it as kana unless it is an explicitly named product.
                let hasUnconvertedLetters = reading.dropLast(reading.last == "n" ? 1 : 0)
                    .contains { $0.isASCII && $0.isLetter }
                if nodes[node].product || hasUnconvertedLetters {
                    // Do not split an unfinished kana syllable: "shit" in
                    // "soshite" is followed by the vowel that completes "て".
                    if !nodes[node].product, candidateEnd < characters.count,
                       "aeiou".contains(characters[candidateEnd]) {
                        let target = romanized(word + String(characters[candidateEnd]), style: style).convertTarget
                        if !target.contains(where: { $0.isASCII && $0.isLetter }) { continue }
                    }
                    let prefix = romanized(String(characters[kanaStart..<start]), style: style).convertTarget
                    guard !prefix.contains(where: { $0.isASCII && $0.isLetter }) else { continue }
                    englishEnd = candidateEnd
                    break
                }
            }
            if let englishEnd {
                result.append(contentsOf: characters[start..<englishEnd].map { .init(character: $0, inputStyle: .direct) })
                start = englishEnd
                kanaStart = englishEnd
            } else {
                result.append(.init(character: characters[start], inputStyle: style))
                start += 1
            }
        }
        return result
    }

    static func update(_ text: inout ComposingText, raw: String, style: InputStyle) {
        // A newly completed dictionary word can change at most 32 previous
        // strokes. Keep earlier input styles and rescan only that tail, starting
        // at a real kana boundary (or the start of a retained English span).
        var offset = 0
        let previous = Self.raw(text)
        if style == .roman2kana, raw.first?.isUppercase != true,
           raw.hasPrefix(previous), previous.count == text.input.count,
           text.input.count > 32 {
            offset = text.inputIndexToSurfaceIndexMap().keys
                .filter { $0 <= text.input.count - 32 }.max() ?? 0
            while offset > 0, offset < text.input.count,
                  text.input[offset].inputStyle == .direct,
                  text.input[offset - 1].inputStyle == .direct { offset -= 1 }
        }
        let next = Array(text.input.prefix(offset)) + elements(String(raw.dropFirst(offset)), style: style)
        let prefix = text.input.count <= next.count && zip(text.input, next).allSatisfy {
            $0.piece == $1.piece && $0.inputStyle == $1.inputStyle
        }
        if prefix {
            text.insertAtCursorPosition(Array(next.dropFirst(text.input.count)))
        } else {
            text = ComposingText()
            text.insertAtCursorPosition(next)
        }
    }

    static func raw(_ text: ComposingText) -> String {
        String(text.input.compactMap {
            if case .character(let character) = $0.piece { return character }
            return nil
        })
    }
}

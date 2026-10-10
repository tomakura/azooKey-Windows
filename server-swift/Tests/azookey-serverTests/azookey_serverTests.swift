import Testing
import Foundation
import KanaKanjiConverterModule
@testable import azookey_server

/// Cover absent and blank paths, bounded UTF-8 input, invalid files, and disabled personalization.
@Test func personalizationCanBeEnabledBeforeSelectingAFile() throws {
    let enabled: [String: Any] = ["enable": true, "personalization": true]
    #expect(try personalizationText(enabled) == "")
    #expect(try personalizationText(enabled.merging(["personalization_path": "  \n"]) { _, new in new }) == "")
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".txt")
    try String(repeating: "猫", count: 5000).write(to: url, atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: url) }
    #expect(try personalizationText(enabled.merging(["personalization_path": url.path]) { _, new in new }) == String(repeating: "猫", count: 4096))
    #expect(throws: (any Error).self) {
        try personalizationText(enabled.merging(["personalization_path": url.path + ".missing"]) { _, new in new })
    }
    #expect(try personalizationText(["enable": false, "personalization": true, "personalization_path": url.path + ".missing"]) == "")
}

/// CRLF, LF, and CR resources must produce the same English words and exclude comments.
@Test func englishDictionaryAcceptsWindowsLineEndings() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = ["# SCOWL", "windows", "meeting", "", "ab", "python"].joined(separator: newline)
        #expect(MixedInput.parseWords(source) == ["windows", "meeting", "python"])
    }
}

@Test func mixedEnglishReading() {
    for (raw, expected) in [
        ("windows", "windows"),
        ("windowswotsukaimasu", "windowsをつかいます"),
        ("kyouhagoogledekensaku", "きょうはgoogleでけんさく"),
        ("ashitameetinggaaru", "あしたmeetingがある"),
        ("pythonnobug", "pythonのbug"),
        ("kyouhagithub", "きょうはgithub"),
        ("soshitegithub", "そしてgithub"),
        ("İgithub", "İgithub"),
        ("arigatougozaimasu", "ありがとうございます"),
        ("konnnichiha", "こんにちは"),
        ("nihongowobenkyoushiteimasu", "にほんごをべんきょうしています"),
        ("make", "まけ"),
        ("sushi", "すし"),
        ("kore", "これ"),
        ("kanji", "かんじ"),
        ("kandou", "かんどう"),
        ("kandannnatesuto", "かんだんなてすと"),
    ] {
        var text = ComposingText()
        var strokes = ""
        for character in raw {
            strokes.append(character)
            MixedInput.update(&text, raw: strokes, style: .roman2kana)
            #expect(MixedInput.raw(text) == strokes)
        }
        #expect(text.convertTarget == expected, "\(raw): \(text.convertTarget)")
        var snapshot = ComposingText()
        MixedInput.update(&snapshot, raw: raw, style: .roman2kana)
        #expect(snapshot.convertTarget == text.convertTarget)
    }
    for raw in [
        "soshite", "hajimemashite", "yoroshikuonegaishimasu", "otsukaresamadesu",
        "ashitahakaishaniikimasu", "shiryouwokakuninshiteimasu", "kaigiwokaishishimasu",
        "kinouhakouenniikimashita", "toukyounoshingou", "kantannasetumeidesu",
        "konngetunoyotei", "shinchokuwokakunin", "seiseki", "watashinoshigoto",
        "toriaezuyattemimasu", "desukutoppu", "pasokon", "sofutowea", "konnnichiwa",
    ] {
        var text = ComposingText()
        MixedInput.update(&text, raw: raw, style: .roman2kana)
        #expect(text.convertTarget == MixedInput.romanized(raw, style: .roman2kana).convertTarget, "\(raw): \(text.convertTarget)")
    }
    for raw in [
        String(repeating: "kyouhaiitenkidesu", count: 12),
        String(repeating: "kyouhaiitenkidesu", count: 3) + "githubwotsukaimasu",
        String(repeating: "pythonnobugwoshirabemasu", count: 8),
    ] {
        var typed = ComposingText()
        var strokes = ""
        for character in raw {
            strokes.append(character)
            MixedInput.update(&typed, raw: strokes, style: .roman2kana)
            var snapshot = ComposingText()
            snapshot.insertAtCursorPosition(MixedInput.elements(strokes, style: .roman2kana))
            #expect(typed.convertTarget == snapshot.convertTarget, "\(strokes)")
            #expect(MixedInput.raw(typed) == strokes)
        }
    }
    var text = ComposingText()
    MixedInput.update(&text, raw: "windowswotsukau", style: .roman2kana)
    text.prefixComplete(composingCount: .surfaceCount(7))
    #expect(text.convertTarget == "をつかう")
    #expect(MixedInput.raw(text) == "wotsukau")
    text.deleteBackwardFromCursorPosition(count: 1)
    MixedInput.update(&text, raw: MixedInput.raw(text) + "i", style: .roman2kana)
    #expect(text.convertTarget == "をつかい")
}

@Test func dictionaryPartOfSpeechAndRequestOptions() {
    let entries = parseUserDictionary("やまだ\t山田\t人名姓\nとうきょう\t東京\t地名\n")
    #expect(entries.count == 2)
    #expect(entries[0].ruby == "ヤマダ")
    #expect(entries[0].lcid == 1290)
    #expect(entries[1].lcid == 1293)
    config["learning"] = false
    config["prediction"] = false
    config["typo_correction"] = false
    config["dynamic_candidates"] = false
    config["enable"] = false
    let options = getOptions()
    #expect(options.learningType == .nothing)
    #expect(options.requireJapanesePrediction == .disabled)
    config["prediction"] = true
    #expect(getOptions().requireJapanesePrediction == .manualMix)
    #expect(options.typoCorrectionMode == .disabled)
    #expect(options.specialCandidateProviders.isEmpty)
    config["enable"] = true
    if case .off = getOptions(predictionOnly: true).zenzaiMode {} else {
        Issue.record("Typing predictions must not run Zenzai")
    }
    config["enable"] = false

    // The reading path must remain usable without creating or invoking a converter.
    func edit(_ text: String, _ operation: Int32 = 0, _ count: Int32 = 0) -> String {
        text.withCString { input in
            let pointer = edit_reading(input: input, operation: operation, count: count)
            defer { free_text(pointer) }
            return String(cString: pointer)
        }
    }
    #expect(edit("", 3) == "")
    for (input, expected) in zip(["W", "i", "n", "d", "o", "w", "s"], ["W", "Wi", "Win", "Wind", "Windo", "Window", "Windows"]) {
        #expect(edit(input) == expected)
    }
    #expect(edit("", 1) == "Window")
    #expect(edit("s") == "Windows")
    #expect(edit("", 3) == "")
    #expect(edit("kanji") == "かんじ")
    let original = get_reading_input()
    #expect(String(cString: original) == "kanji")
    free_text(original)
    #expect(edit("", 2, 2) == "じ")
    #expect(edit("", 3) == "")
}

/// Learning links consecutive commits only when the new input follows the committed text.
@Test func commitsLinkOnlyWhenInputContinuesCommittedText() {
    #expect(continuesCommittedText("昨日は会議で今日は", "今日は"))
    #expect(continuesCommittedText("今日は", "とても長い確定文字列の末尾の今日は"))
    #expect(!continuesCommittedText("別の段落", "今日は"))
    #expect(!continuesCommittedText("", "今日は"))
    #expect(!continuesCommittedText("今日は", ""))
}

import Testing
@testable import azookey_server

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

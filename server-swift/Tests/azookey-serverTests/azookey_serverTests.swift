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
}

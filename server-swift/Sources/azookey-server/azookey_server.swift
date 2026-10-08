import KanaKanjiConverterModule
import Foundation
import ffi

// The Rust server serializes all FFI calls; these functions run on its worker threads.
nonisolated(unsafe) var converter: KanaKanjiConverter?
nonisolated(unsafe) var composingText = ComposingText()
// Reading edits use their own lock in Rust and never touch the converter or its inference state.
nonisolated(unsafe) var readingText = ComposingText()
nonisolated(unsafe) var lastCandidates: [Candidate] = []
nonisolated(unsafe) var inputStyle: InputStyle = .roman2kana
nonisolated(unsafe) var userEntries: [DicdataElement] = []

nonisolated(unsafe) var execURL = URL(filePath: "")
nonisolated(unsafe) var config: [String : Any] = [
    "enable": false,
    "profile": "",
    "runtimeUseZenzai": true,
    "inferenceLimit": 1,
]

func ensureDirectory(_ url: URL) {
    do {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    } catch {
        print("Failed to create directory \(url.path): \(error)")
    }
}

func appSupportURL() -> URL {
    if let appDataPath = ProcessInfo.processInfo.environment["APPDATA"] {
        return URL(filePath: appDataPath).appendingPathComponent("Azookey", isDirectory: true)
    }
    return execURL.appendingPathComponent("Azookey", isDirectory: true)
}

func memoryDirectoryURL() -> URL {
    let url = appSupportURL().appendingPathComponent("memory", isDirectory: true)
    ensureDirectory(url)
    return url
}

func userDictionaryDirectoryURL() -> URL {
    let url = appSupportURL().appendingPathComponent("user_dictionary", isDirectory: true)
    ensureDirectory(url)
    return url
}

func emojiDictionaryURL() -> URL {
    let directory = execURL.appendingPathComponent("EmojiDictionary", isDirectory: true)
    for name in [
        "emoji_all_E16.0.txt",
        "emoji_all_E15.1.txt",
        "emoji_all_E15.0.txt",
        "emoji_all_E14.0.txt",
        "emoji_all_E13.1.txt",
    ] {
        let candidate = directory.appendingPathComponent(name, isDirectory: false)
        if FileManager.default.fileExists(atPath: candidate.path) {
            return candidate
        }
    }
    return directory.appendingPathComponent("emoji_all_E15.1.txt", isDirectory: false)
}

func zenzaiMode(context: String) -> ConvertRequestOptions.ZenzaiMode {
    guard (config["runtimeUseZenzai"] as? Bool) ?? true,
          (config["enable"] as? Bool) ?? false else {
        return .off
    }

    let profile = (config["profile"] as? String) ?? ""
    let personal = (config["personalizationText"] as? String) ?? ""
    let combinedProfile = [profile, personal].filter { !$0.isEmpty }.joined(separator: "\n")
    let modelPath = (config["model_path"] as? String) ?? ""
    return .on(
        weight: modelPath.isEmpty ? execURL.appendingPathComponent("zenz.gguf") : URL(filePath: modelPath),
        inferenceLimit: (config["inferenceLimit"] as? Int) ?? 1,
        requestRichCandidates: true,
        personalizationMode: nil,
        versionDependentMode: .v3(
            .init(
                profile: combinedProfile.isEmpty ? nil : combinedProfile,
                leftSideContext: context.isEmpty ? nil : context,
                enableAlignmentSeparator: true
            )
        )
    )
}

func getOptions(context: String = "", predictionOnly: Bool = false) -> ConvertRequestOptions {
    let emojiURL = emojiDictionaryURL()
    return ConvertRequestOptions(
        N_best: max(1, min(32, (config["max_candidates"] as? Int) ?? 16)),
        requireJapanesePrediction: ((config["prediction"] as? Bool) ?? true) ? .manualMix : .disabled,
        requireEnglishPrediction: .disabled,
        keyboardLanguage: .ja_JP,
        englishCandidateInRoman2KanaInput: true,
        fullWidthRomanCandidate: true,
        learningType: ((config["learning"] as? Bool) ?? true) ? .inputAndOutput : .nothing,
        memoryDirectoryURL: memoryDirectoryURL(),
        sharedContainerURL: userDictionaryDirectoryURL(),
        textReplacer: .init(emojiDataProvider: { emojiURL }),
        specialCandidateProviders: ((config["dynamic_candidates"] as? Bool) ?? true)
            ? KanaKanjiConverter.defaultSpecialCandidateProviders : [],
        zenzaiMode: predictionOnly && !((config["live_conversion"] as? Bool) ?? false) ? .off : zenzaiMode(context: context),
        preloadDictionary: true,
        experimentalZenzaiPredictiveInput: (!predictionOnly || ((config["live_conversion"] as? Bool) ?? false)) && ((config["zenzai_prediction"] as? Bool) ?? false)
            && ((config["prediction"] as? Bool) ?? true),
        typoCorrectionMode: ((config["typo_correction"] as? Bool) ?? true) ? .enabled : .disabled,
        metadata: .init(versionString: "Azookey for Windows")
    )
}

class SimpleComposingText {
    init(text: String, cursor: Int) {
        self.text = UnsafeMutablePointer<CChar>(mutating: text.utf8String)!
        self.cursor = cursor
    }

    var text: UnsafeMutablePointer<CChar>
    var cursor: Int
}

struct SComposingText {
    var text: UnsafeMutablePointer<CChar>
    var cursor: Int
}

func constructCandidateString(candidate: Candidate, hiragana _: String) -> String {
    return candidate.text
}

@_silgen_name("LoadConfig")
public func load_config() -> UnsafeMutablePointer<CChar> {
    do {
        let data = try Data(contentsOf: appSupportURL().appendingPathComponent("settings.json"))
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return _strdup("Settings must be a JSON object")!
        }
        let zenzai = (json["zenzai"] as? [String: Any]) ?? [:]
        let conversion = (json["conversion"] as? [String: Any]) ?? [:]
        var next = config
        next["enable"] = (zenzai["enable"] as? Bool) ?? false
        next["profile"] = (zenzai["profile"] as? String) ?? ""
        next["model_path"] = (zenzai["model_path"] as? String) ?? ""
        next["inferenceLimit"] = (zenzai["inference_limit"] as? Int) ?? 2
        next["zenzai_prediction"] = (zenzai["prediction"] as? Bool) ?? false
        next["learning"] = ((json["learning"] as? [String: Any])?["enable"] as? Bool) ?? true
        for key in ["prediction", "typo_correction", "dynamic_candidates", "max_candidates", "live_conversion"] {
            next[key] = conversion[key]
        }
        next["personalizationText"] = ""
        if ((zenzai["enable"] as? Bool) ?? false) && ((zenzai["personalization"] as? Bool) ?? false) {
            let path = (zenzai["personalization_path"] as? String) ?? ""
            let text = try String(contentsOf: URL(filePath: path), encoding: .utf8)
            next["personalizationText"] = String(text.prefix(4096))
        }
        let style = (conversion["input_style"] as? String) ?? "default"
        var nextStyle: InputStyle = .roman2kana
        if style == "azik" {
            nextStyle = .mapped(id: .defaultAZIK)
        } else if style == "custom" {
            let path = (conversion["custom_input_table_path"] as? String) ?? ""
            let url = path.isEmpty ? appSupportURL().appendingPathComponent("input_table.tsv") : URL(filePath: path)
            let custom = try String(contentsOf: url, encoding: .utf8)
            let standard = try InputStyleManager.exportTable(.defaultRomanToKana)
            let mergedURL = appSupportURL().appendingPathComponent("input_table_merged.tsv")
            try (standard + "\n" + custom).write(to: mergedURL, atomically: true, encoding: .utf8)
            let table = try InputStyleManager.loadTable(from: mergedURL)
            InputStyleManager.registerInputStyle(table: table, for: "windows-custom")
            nextStyle = .mapped(id: .tableName("windows-custom"))
        }
        let dictionaryURL = appSupportURL().appendingPathComponent("user_dictionary.tsv")
        let entries = FileManager.default.fileExists(atPath: dictionaryURL.path)
            ? try parseUserDictionary(String(contentsOf: dictionaryURL, encoding: .utf8)) : []
        config = next
        inputStyle = nextStyle
        composingText = ComposingText()
        converter?.stopComposition()
        userEntries = entries
        converter?.importDynamicUserDictionary(entries)
        return _strdup("")!
    } catch {
        return _strdup("Failed to load configuration: \(error)")!
    }
}

func toKatakana(_ text: String) -> String {
    String(String.UnicodeScalarView(text.unicodeScalars.map { scalar in
        (0x3041...0x3096).contains(scalar.value) ? UnicodeScalar(scalar.value + 0x60)! : scalar
    }))
}

func parseUserDictionary(_ text: String) -> [DicdataElement] {
    text.split(whereSeparator: \.isNewline).compactMap { line in
        let fields = line.split(separator: "\t", omittingEmptySubsequences: false)
        guard fields.count >= 2, !fields[0].isEmpty, !fields[1].isEmpty else { return nil }
        let part = fields.count > 2 ? String(fields[2]) : ""
        let cid: CIDData = switch part {
        case "固有名詞": .固有名詞
        case "人名", "人名一般": .人名一般
        case "姓", "人名姓": .人名姓
        case "名", "人名名": .人名名
        case "地名", "地名一般": .地名一般
        case "組織", "固有名詞組織": .固有名詞組織
        case "数", "数詞": .数
        case "記号": .記号
        default: .一般名詞
        }
        return DicdataElement(word: String(fields[1]), ruby: toKatakana(String(fields[0])),
            cid: cid.cid, mid: MIDData.一般.mid, value: -5, metadata: .isFromUserDictionary)
    }
}

@_silgen_name("Initialize")
public func initialize(
    path: UnsafePointer<CChar>,
    use_zenzai: Bool
) -> UnsafeMutablePointer<CChar> {
    let path = String(cString: path)
    execURL = URL(filePath: path)
    config["runtimeUseZenzai"] = use_zenzai

    let error = load_config()
    if error.pointee != 0 { return error }
    free(error)
    ensureDirectory(appSupportURL())

    converter = KanaKanjiConverter(
        dictionaryURL: execURL.appendingPathComponent("Dictionary", isDirectory: true),
        preloadDictionary: true
    )
    converter?.setKeyboardLanguage(.ja_JP)
    converter?.importDynamicUserDictionary(userEntries)

    composingText.insertAtCursorPosition("a", inputStyle: .roman2kana)
    _ = converter?.requestCandidates(composingText, options: getOptions())
    composingText = ComposingText()
    return _strdup("")!
}

@_silgen_name("AppendText")
public func append_text(
    input: UnsafePointer<CChar>,
    cursorPtr: UnsafeMutablePointer<Int32>
) -> UnsafeMutablePointer<CChar> {
    let inputString = String(cString: input)
    composingText.insertAtCursorPosition(inputString, inputStyle: inputStyle)

    cursorPtr.pointee = Int32(composingText.convertTargetCursorPosition)
    return _strdup(composingText.convertTarget)!
}

@_silgen_name("EditReading")
public func edit_reading(input: UnsafePointer<CChar>, operation: Int32, count: Int32) -> UnsafeMutablePointer<CChar> {
    switch operation {
    case 0:
        let text = String(cString: input)
        let first = readingText.convertTarget.first ?? text.first
        let latin = first.map { $0.isASCII && $0.isUppercase } ?? false
        readingText.insertAtCursorPosition(text, inputStyle: latin ? .direct : inputStyle)
    case 1: readingText.deleteBackwardFromCursorPosition(count: 1)
    case 2: readingText.prefixComplete(composingCount: .surfaceCount(Int(count)))
    case 3: readingText = ComposingText()
    default: preconditionFailure("Unknown reading operation")
    }
    return _strdup(readingText.convertTarget)!
}

@_silgen_name("GetReadingInput")
public func get_reading_input() -> UnsafeMutablePointer<CChar> {
    let text = readingText.input.compactMap { element -> Character? in
        if case .character(let character) = element.piece { return character }
        return nil
    }
    return _strdup(String(text))!
}

@_silgen_name("RemoveText")
public func remove_text(
    cursorPtr: UnsafeMutablePointer<Int32>
) -> UnsafeMutablePointer<CChar> {
    composingText.deleteBackwardFromCursorPosition(count: 1)

    cursorPtr.pointee = Int32(composingText.convertTargetCursorPosition)
    return _strdup(composingText.convertTarget)!
}

@_silgen_name("MoveCursor")
public func move_cursor(
    offset: Int32,
    cursorPtr: UnsafeMutablePointer<Int32>
) -> UnsafeMutablePointer<CChar> {
    let cursor = composingText.moveCursorFromCursorPosition(count: Int(offset))

    cursorPtr.pointee = Int32(cursor)
    return _strdup(composingText.convertTarget)!
}

@_silgen_name("ClearText")
public func clear_text() {
    composingText = ComposingText()
    lastCandidates = []
    converter?.stopComposition()
}

@_silgen_name("ResetLearning")
public func reset_learning() {
    converter?.resetMemory()
}

@_silgen_name("CommitCandidate")
public func commit_candidate(reading: UnsafePointer<CChar>, text: UnsafePointer<CChar>) {
    guard (config["learning"] as? Bool) ?? true, let converter else { return }
    let ruby = toKatakana(String(cString: reading))
    let word = String(cString: text)
    let candidate = lastCandidates.first {
        $0.text == word && $0.data.map(\.ruby).joined() == ruby
    } ?? Candidate(text: word, value: -5, composingCount: .surfaceCount(ruby.count),
        lastMid: MIDData.一般.mid,
        data: [DicdataElement(word: word, ruby: ruby, cid: CIDData.一般名詞.cid, mid: MIDData.一般.mid, value: -5)])
    converter.setCompletedData(candidate)
    converter.updateLearningData(candidate)
    converter.commitUpdateLearningData()
}

@_silgen_name("FreeText")
public func free_text(_ pointer: UnsafeMutablePointer<CChar>) { free(pointer) }

@_silgen_name("GetZenzaiStatus")
public func get_zenzai_status() -> UnsafeMutablePointer<CChar> {
    _strdup(converter?.zenzStatus ?? "")!
}

@_silgen_name("FreeCandidates")
public func free_candidates(_ pointer: UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?>, _ count: Int32) {
    for index in 0..<Int(count) {
        if let item = pointer[index] {
            free(item.pointee.text)
            free(item.pointee.subtext)
            free(item.pointee.hiragana)
            item.deinitialize(count: 1)
            item.deallocate()
        }
    }
    pointer.deinitialize(count: Int(count))
    pointer.deallocate()
}

func to_list_pointer(_ list: [FFICandidate]) -> UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?> {
    let pointer = UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?>.allocate(capacity: list.count)
    for (i, item) in list.enumerated() {
        let entry = UnsafeMutablePointer<FFICandidate>.allocate(capacity: 1)
        entry.initialize(to: item)
        pointer.advanced(by: i).initialize(to: entry)
    }
    return pointer
}

@_silgen_name("GetComposedText")
public func get_composed_text(lengthPtr: UnsafeMutablePointer<Int32>) -> UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?> {
    collect_candidates(lengthPtr: lengthPtr, predictionOnly: false)
}

@_silgen_name("GetSnapshotCandidates")
public func get_snapshot_candidates(input: UnsafePointer<CChar>, rawInput: UnsafePointer<CChar>, predictionOnly: Bool, lengthPtr: UnsafeMutablePointer<Int32>) -> UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?> {
    let raw = String(cString: rawInput)
    let first = raw.first
    let latin = first.map { $0.isASCII && $0.isUppercase } ?? false
    let next = raw.isEmpty ? String(cString: input) : raw
    let previous = String(composingText.input.compactMap { element -> Character? in
        if case .character(let character) = element.piece { return character }
        return nil
    })
    if next.hasPrefix(previous) {
        composingText.insertAtCursorPosition(String(next.dropFirst(previous.count)),
            inputStyle: raw.isEmpty || latin ? .direct : inputStyle)
    } else {
        converter?.stopComposition()
        composingText = ComposingText()
        composingText.insertAtCursorPosition(next, inputStyle: raw.isEmpty || latin ? .direct : inputStyle)
    }
    return collect_candidates(lengthPtr: lengthPtr, predictionOnly: predictionOnly, normalOnly: !predictionOnly)
}

func collect_candidates(lengthPtr: UnsafeMutablePointer<Int32>, predictionOnly: Bool, normalOnly: Bool = false) -> UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?> {
    let hiragana = composingText.convertTarget
    let contextString = (config["context"] as? String) ?? ""
    var options = getOptions(context: contextString, predictionOnly: predictionOnly)
    if normalOnly {
        options.requireJapanesePrediction = .disabled
        options.experimentalZenzaiPredictiveInput = false
    }
    guard let converter else {
        lengthPtr.pointee = 0
        return to_list_pointer([])
    }
    let converted = converter.requestCandidates(composingText, options: options)
    lastCandidates = converted.mainResults + converted.predictionResults
    // Registered words take priority while remaining available to compound conversion.
    let katakana = toKatakana(hiragana)
    let matched = userEntries.filter { katakana.hasPrefix($0.ruby) }
    let registered = matched.map { entry in
        Candidate(text: entry.word, value: -5, composingCount: .surfaceCount(entry.ruby.count),
            lastMid: entry.mid, data: [entry])
    }
    // Keep predictions separate from conversions so live preview never completes untyped text.
    let limit = max(1, min(100, (config["max_candidates"] as? Int) ?? 16))
    let predictions = Array(converted.predictionResults.prefix(predictionOnly ? limit : max(0, limit - 1)))
    let candidates = predictionOnly ? predictions.map { ($0, true) }
        : (registered + converted.mainResults).prefix(limit - predictions.count).map { ($0, false) }
            + predictions.map { ($0, true) }
    var result: [FFICandidate] = []
    var seen: Set<String> = []

    for (candidate, isPrediction) in candidates {
        var afterComposingText = composingText
        afterComposingText.prefixComplete(composingCount: candidate.composingCount)
        let correspondingCount = composingText.convertTarget.count - afterComposingText.convertTarget.count
        guard seen.insert("\(candidate.text)\t\(correspondingCount)").inserted else { continue }
        let text = _strdup(constructCandidateString(candidate: candidate, hiragana: hiragana))
        let hiragana = _strdup(hiragana)
        let subtext = _strdup(afterComposingText.convertTarget)

        result.append(FFICandidate(text: text, subtext: subtext, hiragana: hiragana, correspondingCount: Int32(correspondingCount), isPrediction: isPrediction ? 1 : 0))
        if result.count >= limit { break }
    }

    lengthPtr.pointee = Int32(result.count)

    return to_list_pointer(result)
}

@_silgen_name("ShrinkText")
public func shrink_text(
    offset: Int32
) -> UnsafeMutablePointer<CChar>  {
    var afterComposingText = composingText
    afterComposingText.prefixComplete(composingCount: .surfaceCount(Int(offset)))
    composingText = afterComposingText

    return _strdup(composingText.convertTarget)!
}

@_silgen_name("SetContext")
public func set_context(
    context: UnsafePointer<CChar>
) {
    let contextString = String(cString: context)
    config["context"] = contextString
}

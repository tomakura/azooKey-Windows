import KanaKanjiConverterModule
import Foundation
import ffi

@MainActor var converter: KanaKanjiConverter?
@MainActor var composingText = ComposingText()

@MainActor var execURL = URL(filePath: "")
@MainActor var config: [String : Any] = [
    "enable": false,
    "profile": "",
    "runtimeUseZenzai": true,
    "inferenceLimit": 1,
]

@MainActor func ensureDirectory(_ url: URL) {
    do {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    } catch {
        print("Failed to create directory \(url.path): \(error)")
    }
}

@MainActor func appSupportURL() -> URL {
    if let appDataPath = ProcessInfo.processInfo.environment["APPDATA"] {
        return URL(filePath: appDataPath).appendingPathComponent("Azookey", isDirectory: true)
    }
    return execURL.appendingPathComponent("Azookey", isDirectory: true)
}

@MainActor func memoryDirectoryURL() -> URL {
    let url = appSupportURL().appendingPathComponent("memory", isDirectory: true)
    ensureDirectory(url)
    return url
}

@MainActor func userDictionaryDirectoryURL() -> URL {
    let url = appSupportURL().appendingPathComponent("user_dictionary", isDirectory: true)
    ensureDirectory(url)
    return url
}

@MainActor func emojiDictionaryURL() -> URL {
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

@MainActor func zenzaiMode(context: String) -> ConvertRequestOptions.ZenzaiMode {
    guard (config["runtimeUseZenzai"] as? Bool) ?? true,
          (config["enable"] as? Bool) ?? false else {
        return .off
    }

    let profile = (config["profile"] as? String) ?? ""
    return .on(
        weight: execURL.appendingPathComponent("zenz.gguf", isDirectory: false),
        inferenceLimit: (config["inferenceLimit"] as? Int) ?? 1,
        requestRichCandidates: true,
        personalizationMode: nil,
        versionDependentMode: .v3(
            .init(
                profile: profile.isEmpty ? nil : profile,
                leftSideContext: context.isEmpty ? nil : context,
                enableAlignmentSeparator: true
            )
        )
    )
}

@MainActor func getOptions(context: String = "") -> ConvertRequestOptions {
    let emojiURL = emojiDictionaryURL()
    return ConvertRequestOptions(
        requireJapanesePrediction: .autoMix,
        requireEnglishPrediction: .disabled,
        keyboardLanguage: .ja_JP,
        englishCandidateInRoman2KanaInput: false,
        fullWidthRomanCandidate: true,
        learningType: .nothing,
        memoryDirectoryURL: memoryDirectoryURL(),
        sharedContainerURL: userDictionaryDirectoryURL(),
        textReplacer: .init(emojiDataProvider: { emojiURL }),
        specialCandidateProviders: KanaKanjiConverter.defaultSpecialCandidateProviders,
        zenzaiMode: zenzaiMode(context: context),
        preloadDictionary: true,
        experimentalZenzaiPredictiveInput: false,
        typoCorrectionMode: .automatic,
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
@MainActor public func load_config() {
    if let appDataPath = ProcessInfo.processInfo.environment["APPDATA"] {
        let settingsPath = URL(filePath: appDataPath).appendingPathComponent("Azookey/settings.json")
        
        do {
            let data = try Data(contentsOf: settingsPath)
            if let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
               let zenzaiDict = json["zenzai"] as? [String: Any] {
                
                if let enableValue = zenzaiDict["enable"] as? Bool {
                    config["enable"] = enableValue
                }
                
                if let profileValue = zenzaiDict["profile"] as? String {
                    config["profile"] = profileValue
                }

                if let inferenceLimitValue = zenzaiDict["inference_limit"] as? Int {
                    config["inferenceLimit"] = inferenceLimitValue
                }
            }
        } catch {
            print("Failed to read settings: \(error)")
        }
    }
}

@_silgen_name("Initialize")
@MainActor public func initialize(
    path: UnsafePointer<CChar>,
    use_zenzai: Bool
) {
    let path = String(cString: path)
    execURL = URL(filePath: path)
    config["runtimeUseZenzai"] = use_zenzai

    load_config()
    ensureDirectory(appSupportURL())

    converter = KanaKanjiConverter(
        dictionaryURL: execURL.appendingPathComponent("Dictionary", isDirectory: true),
        preloadDictionary: true
    )
    converter?.setKeyboardLanguage(.ja_JP)

    composingText.insertAtCursorPosition("a", inputStyle: .roman2kana)
    _ = converter?.requestCandidates(composingText, options: getOptions())
    composingText = ComposingText()
}

@_silgen_name("AppendText")
@MainActor public func append_text(
    input: UnsafePointer<CChar>,
    cursorPtr: UnsafeMutablePointer<Int>
) -> UnsafeMutablePointer<CChar> {
    let inputString = String(cString: input)
    composingText.insertAtCursorPosition(inputString, inputStyle: .roman2kana)

    cursorPtr.pointee = composingText.convertTargetCursorPosition    
    return _strdup(composingText.convertTarget)!
}

@_silgen_name("RemoveText")
@MainActor public func remove_text(
    cursorPtr: UnsafeMutablePointer<Int>
) -> UnsafeMutablePointer<CChar> {
    composingText.deleteBackwardFromCursorPosition(count: 1)

    cursorPtr.pointee = composingText.convertTargetCursorPosition
    return _strdup(composingText.convertTarget)!
}

@_silgen_name("MoveCursor")
@MainActor public func move_cursor(
    offset: Int32,
    cursorPtr: UnsafeMutablePointer<Int>
) -> UnsafeMutablePointer<CChar> {
    let previousCursor = composingText.convertTargetCursorPosition
    let cursor = composingText.moveCursorFromCursorPosition(count: Int(offset))
    print("offset: \(offset), cursor: \(cursor)")

    cursorPtr.pointee = cursor
    return _strdup(composingText.convertTarget)!
}

@_silgen_name("ClearText")
@MainActor public func clear_text() {
    composingText = ComposingText()
    converter?.stopComposition()
}

func to_list_pointer(_ list: [FFICandidate]) -> UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?> {
    let pointer = UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?>.allocate(capacity: list.count)
    for (i, item) in list.enumerated() {
        pointer[i] = UnsafeMutablePointer<FFICandidate>.allocate(capacity: 1)
        pointer[i]?.pointee = item
    }
    return pointer
}

@_silgen_name("GetComposedText")
@MainActor public func get_composed_text(lengthPtr: UnsafeMutablePointer<Int>) -> UnsafeMutablePointer<UnsafeMutablePointer<FFICandidate>?> {
    let hiragana = composingText.convertTarget
    let contextString = (config["context"] as? String) ?? ""
    let options = getOptions(context: contextString)
    guard let converter else {
        lengthPtr.pointee = 0
        return to_list_pointer([])
    }
    let converted = converter.requestCandidates(composingText, options: options)
    var result: [FFICandidate] = []

    for i in 0..<converted.mainResults.count {
        let candidate = converted.mainResults[i]

        let text = strdup(constructCandidateString(candidate: candidate, hiragana: hiragana))
        let hiragana = strdup(hiragana)

        var afterComposingText = composingText
        afterComposingText.prefixComplete(composingCount: candidate.composingCount)
        let correspondingCount = composingText.convertTarget.count - afterComposingText.convertTarget.count
        let subtext = strdup(afterComposingText.convertTarget)

        result.append(FFICandidate(text: text, subtext: subtext, hiragana: hiragana, correspondingCount: Int32(correspondingCount)))        
    }

    lengthPtr.pointee = result.count

    return to_list_pointer(result)
}

@_silgen_name("ShrinkText")
@MainActor public func shrink_text(
    offset: Int32
) -> UnsafeMutablePointer<CChar>  {
    var afterComposingText = composingText
    afterComposingText.prefixComplete(composingCount: .surfaceCount(Int(offset)))
    composingText = afterComposingText

    return _strdup(composingText.convertTarget)!
}

@_silgen_name("SetContext")
@MainActor public func set_context(
    context: UnsafePointer<CChar>
) {
    let contextString = String(cString: context)
    config["context"] = contextString
}

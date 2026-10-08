export type UserDictionaryEntry = {
    reading: string;
    text: string;
    part_of_speech: string;
};

export function parseDictionary(content: string): UserDictionaryEntry[] {
    const lines = content.replace(/^\uFEFF/, "").split(/\r?\n/);
    return lines.flatMap((line, index) => {
        if (!line.trim() || line.trimStart().startsWith("#")) return [];
        const separator = line.includes("\t") ? "\t" : ",";
        const fields: string[] = [];
        let field = "";
        let quoted = false;
        for (let i = 0; i < line.length; i++) {
            const char = line[i];
            if (char === '"' && separator === ",") {
                if (quoted && line[i + 1] === '"') { field += '"'; i++; }
                else quoted = !quoted;
            } else if (char === separator && !quoted) {
                fields.push(field.trim());
                field = "";
            } else field += char;
        }
        fields.push(field.trim());
        if (quoted || fields.length < 2 || fields.length > 3 || !fields[0] || !fields[1]) {
            throw new Error(`${index + 1}行目の形式が正しくありません`);
        }
        return [{ reading: fields[0], text: fields[1], part_of_speech: fields[2] ?? "" }];
    });
}

export function exportDictionary(entries: UserDictionaryEntry[]): string {
    return entries.map((entry, index) => {
        if (!entry.reading.trim() || !entry.text.trim() ||
            [entry.reading, entry.text, entry.part_of_speech].some((field) => /[\t\r\n]/.test(field))) {
            throw new Error(`${index + 1}件目の読み・候補を確認してください`);
        }
        return `${entry.reading.trim()}\t${entry.text.trim()}\t${entry.part_of_speech.trim()}`;
    }).join("\r\n") + "\r\n";
}

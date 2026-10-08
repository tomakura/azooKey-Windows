import assert from "node:assert/strict";
import test from "node:test";
import { parseDictionary, exportDictionary } from "../src/lib/user-dictionary.ts";

test("quoted CSV, BOM and comments import without losing punctuation", () => {
    const entries = parseDictionary('\uFEFF# comment\r\nかんじ,"漢字,表記",普通名詞\r\n');
    assert.deepEqual(entries, [{ reading: "かんじ", text: "漢字,表記", part_of_speech: "普通名詞" }]);
});

test("TSV export imports with identical readings, words, quotes and parts of speech", () => {
    const entries = [
        { reading: "やまだ", text: "山田", part_of_speech: "人名姓" },
        { reading: "よみ", text: '"引用",表記', part_of_speech: "" },
    ];
    assert.deepEqual(parseDictionary(exportDictionary(entries)), entries);
});

test("invalid rows fail instead of being silently dropped", () => {
    assert.throws(() => parseDictionary("bad row"));
    assert.throws(() => parseDictionary('よみ,"unterminated'));
    assert.throws(() => parseDictionary("よみ\t\t普通名詞"));
});

test("incomplete or multiline entries cannot be exported", () => {
    assert.throws(() => exportDictionary([{ reading: "", text: "候補", part_of_speech: "" }]));
    assert.throws(() => exportDictionary([{ reading: "よみ", text: "候補\n破損", part_of_speech: "" }]));
});

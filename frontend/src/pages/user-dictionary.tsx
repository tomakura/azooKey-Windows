import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { BookOpenText, Plus, Save, Trash2, Upload } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

type UserDictionaryEntry = {
    reading: string;
    text: string;
    part_of_speech: string;
};

export const UserDictionary = () => {
    const [entries, setEntries] = useState<UserDictionaryEntry[]>([]);
    const [bulkText, setBulkText] = useState("");

    useEffect(() => {
        invoke<UserDictionaryEntry[]>("get_user_dictionary")
            .then(setEntries)
            .catch(() => toast("ユーザー辞書の読み込みに失敗しました"));
    }, []);

    const updateEntry = (index: number, key: keyof UserDictionaryEntry, value: string) => {
        setEntries((prev) =>
            prev.map((entry, currentIndex) =>
                currentIndex === index ? { ...entry, [key]: value } : entry,
            ),
        );
    };

    const addEntry = () => {
        setEntries((prev) => [...prev, { reading: "", text: "", part_of_speech: "" }]);
    };

    const removeEntry = (index: number) => {
        setEntries((prev) => prev.filter((_, currentIndex) => currentIndex !== index));
    };

    const saveEntries = async () => {
        try {
            await invoke("update_user_dictionary", { entries });
            toast("ユーザー辞書を保存しました");
        } catch {
            toast("ユーザー辞書の保存に失敗しました");
        }
    };

    const importBulkEntries = () => {
        const imported = bulkText
            .split(/\r?\n/)
            .map((line) => line.trim())
            .filter((line) => line && !line.startsWith("#"))
            .map((line) => {
                const fields = line.includes("\t") ? line.split("\t") : line.split(",");
                return {
                    reading: fields[0]?.trim() ?? "",
                    text: fields[1]?.trim() ?? "",
                    part_of_speech: fields[2]?.trim() ?? "",
                };
            })
            .filter((entry) => entry.reading && entry.text);

        if (imported.length === 0) {
            toast("取り込める行がありません");
            return;
        }

        setEntries((prev) => {
            const seen = new Set(prev.map((entry) => `${entry.reading}\t${entry.text}`));
            const next = [...prev];
            for (const entry of imported) {
                const key = `${entry.reading}\t${entry.text}`;
                if (!seen.has(key)) {
                    seen.add(key);
                    next.push(entry);
                }
            }
            return next;
        });
        setBulkText("");
        toast(`${imported.length}件を取り込みました`);
    };

    return (
        <div className="space-y-8">
            <section className="space-y-2">
                <h1 className="text-sm font-bold text-foreground">ユーザー辞書</h1>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <BookOpenText />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            変換候補
                        </p>
                        <p className="text-xs text-muted-foreground">
                            読みと候補を追加すると、システム辞書と学習より優先して表示されます
                        </p>
                    </div>
                    <Button variant="secondary" onClick={addEntry}>
                        <Plus />
                        追加
                    </Button>
                    <Button onClick={saveEntries}>
                        <Save />
                        保存
                    </Button>
                </div>
                <div className="space-y-2 rounded-md border p-4">
                    <div className="space-y-2 rounded-md border border-dashed p-3">
                        <div className="flex items-center gap-3">
                            <Upload className="size-4" />
                            <div className="flex-1 space-y-1">
                                <p className="text-sm font-medium leading-none">
                                    一括取り込み
                                </p>
                                <p className="text-xs text-muted-foreground">
                                    {"読み,候補,品詞 または 読み<TAB>候補<TAB>品詞 の行を追加します"}
                                </p>
                            </div>
                            <Button variant="secondary" onClick={importBulkEntries}>
                                取り込み
                            </Button>
                        </div>
                        <Textarea
                            value={bulkText}
                            placeholder={"かんじ\t漢字\t普通名詞"}
                            onChange={(event) => setBulkText(event.target.value)}
                        />
                    </div>
                    <div className="grid grid-cols-[1fr_1fr_0.8fr_2.5rem] gap-2 px-1 text-xs text-muted-foreground">
                        <span>読み</span>
                        <span>候補</span>
                        <span>品詞</span>
                        <span />
                    </div>
                    {entries.map((entry, index) => (
                        <div key={index} className="grid grid-cols-[1fr_1fr_0.8fr_2.5rem] gap-2">
                            <Input
                                value={entry.reading}
                                placeholder="かんじ"
                                onChange={(event) => updateEntry(index, "reading", event.target.value)}
                            />
                            <Input
                                value={entry.text}
                                placeholder="漢字"
                                onChange={(event) => updateEntry(index, "text", event.target.value)}
                            />
                            <Input
                                value={entry.part_of_speech}
                                placeholder="普通名詞"
                                onChange={(event) => updateEntry(index, "part_of_speech", event.target.value)}
                            />
                            <Button
                                aria-label="削除"
                                variant="ghost"
                                size="icon"
                                onClick={() => removeEntry(index)}
                            >
                                <Trash2 />
                            </Button>
                        </div>
                    ))}
                    {entries.length === 0 && (
                        <div className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
                            登録されている候補はありません
                        </div>
                    )}
                </div>
            </section>
        </div>
    );
};

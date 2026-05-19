import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { BookOpenText, Plus, Save, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

type UserDictionaryEntry = {
    reading: string;
    text: string;
};

export const UserDictionary = () => {
    const [entries, setEntries] = useState<UserDictionaryEntry[]>([]);

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
        setEntries((prev) => [...prev, { reading: "", text: "" }]);
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
                    <div className="grid grid-cols-[1fr_1fr_2.5rem] gap-2 px-1 text-xs text-muted-foreground">
                        <span>読み</span>
                        <span>候補</span>
                        <span />
                    </div>
                    {entries.map((entry, index) => (
                        <div key={index} className="grid grid-cols-[1fr_1fr_2.5rem] gap-2">
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

import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { invoke } from "@tauri-apps/api/core";
import { FileCode, Palette } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

export const Appearance = () => {
    const [value, setValue] = useState({
        background_color: "#FFFFFF",
        accent_color: "#2CB5FF",
        text_color: "#000000",
        custom_css: "",
        custom_css_enabled: false,
    });

    useEffect(() => {
        invoke<any>("get_config")
            .then((data) => {
                setValue({
                    background_color: data.appearance?.background_color ?? "#FFFFFF",
                    accent_color: data.appearance?.accent_color ?? "#2CB5FF",
                    text_color: data.appearance?.text_color ?? "#000000",
                    custom_css: data.appearance?.custom_css ?? "",
                    custom_css_enabled: data.appearance?.custom_css_enabled ?? false,
                });
            })
            .catch(() => {});
    }, []);

    const updateAppearance = async (patch: Partial<typeof value>) => {
        try {
            const data = await invoke<any>("get_config");
            data.appearance = { ...value, ...patch };
            await invoke("update_config", { newConfig: data });
            setValue((prev) => ({ ...prev, ...patch }));
        } catch {
            toast("設定の更新に失敗しました");
        }
    };

    return (
        <div className="space-y-8">
            <section className="space-y-2">
                <h1 className="text-sm font-bold text-foreground">テーマ</h1>
                <div className="flex items-start gap-x-4 pb-8">
                    <div
                        className="candidate-main"
                        style={{
                            backgroundColor: value.background_color,
                            color: value.text_color,
                            borderColor: value.accent_color,
                        }}
                    >
                        <ol className="candidate-ol">
                            <li
                                className="candidate-li"
                                data-selected
                                style={{
                                    backgroundColor: value.accent_color + "33",
                                    outlineColor: value.accent_color,
                                }}
                            >
                                変換候補1
                            </li>
                            <li className="candidate-li">変換候補2</li>
                            <li className="candidate-li">変換候補3</li>
                        </ol>
                        <footer className="candidate-footer">
                            <svg width="20" height="14" viewBox="0 0 22 16" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M3.5 8C4.59202 9.04403 7.54398 10.3978 13.5068 9.93754M1.25349 5.39919C2.77722 0.413397 8.08911 0.79692 10.9673 1.24436C14.2687 1.71311 20.8969 3.82675 20.9985 8.53129C21.1255 14.412 13.1894 15.3069 10.0784 14.9233C6.96748 14.5398 -0.46071 13.0696 1.25349 5.39919Z" stroke="#838384" strokeWidth="1.5" strokeLinecap="round"/>
                            </svg>
                        </footer>
                    </div>
                    <div
                        className="border w-16 h-16 rounded-md flex items-center justify-center text-xl"
                        style={{
                            backgroundColor: value.background_color,
                            borderColor: value.accent_color,
                            color: value.text_color,
                        }}
                    >
                        あ
                    </div>
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Palette />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">背景色</p>
                    </div>
                    <input
                        type="color"
                        value={value.background_color}
                        onChange={(e) => updateAppearance({ background_color: e.target.value })}
                        className="w-8 h-8 rounded-full cursor-pointer border-0 p-0"
                    />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Palette />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">アクセントカラー</p>
                    </div>
                    <input
                        type="color"
                        value={value.accent_color}
                        onChange={(e) => updateAppearance({ accent_color: e.target.value })}
                        className="w-8 h-8 rounded-full cursor-pointer border-0 p-0"
                    />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Palette />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">テキストの色</p>
                    </div>
                    <input
                        type="color"
                        value={value.text_color}
                        onChange={(e) => updateAppearance({ text_color: e.target.value })}
                        className="w-8 h-8 rounded-full cursor-pointer border-0 p-0"
                    />
                </div>
                <div className="space-y-2 rounded-md border p-4">
                    <div className="flex items-center space-x-4">
                        <FileCode />
                        <div className="flex-1 space-y-1">
                            <p className="text-sm font-medium leading-none">カスタムCSS</p>
                            <p className="text-xs text-muted-foreground">
                                有効にした場合、上記の色設定は無視されます
                            </p>
                        </div>
                        <Switch
                            checked={value.custom_css_enabled}
                            onCheckedChange={(checked) => updateAppearance({ custom_css_enabled: checked })}
                        />
                    </div>
                    {value.custom_css_enabled && (
                        <Textarea
                            value={value.custom_css}
                            placeholder={"main { background-color: #1a1a1a; }"}
                            onChange={(e) => setValue((prev) => ({ ...prev, custom_css: e.target.value }))}
                            onBlur={() => updateAppearance({ custom_css: value.custom_css })}
                            className="font-mono text-xs"
                            rows={6}
                        />
                    )}
                </div>
            </section>
        </div>
    );
};

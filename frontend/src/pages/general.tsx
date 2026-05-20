import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { invoke } from "@tauri-apps/api/core";
import { BrainCircuit, ExternalLink, Keyboard, Lightbulb, RefreshCcw, SpellCheck, Trash2, WandSparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

export const General = () => {
    const [value, setValue] = useState({
        learning: true,
        live_conversion: true,
        prediction: true,
        typo_correction: true,
        input_style: "default",
        custom_input_table_path: "",
        candidate_number_selection: true,
        symbol_input_style: "japanese",
    });

    useEffect(() => {
        invoke<any>("get_config")
            .then((data) => {
                setValue({
                    learning: data.learning?.enable ?? true,
                    live_conversion: data.conversion?.live_conversion ?? true,
                    prediction: data.conversion?.prediction ?? true,
                    typo_correction: data.conversion?.typo_correction ?? true,
                    input_style: data.conversion?.input_style ?? "default",
                    custom_input_table_path: data.conversion?.custom_input_table_path ?? "",
                    candidate_number_selection: data.conversion?.candidate_number_selection ?? true,
                    symbol_input_style: data.conversion?.symbol_input_style ?? "japanese",
                });
            })
            .catch(() => {
                toast("設定の読み込みに失敗しました");
            });
    }, []);

    const updateConfig = async (updater: (config: any) => void) => {
        try {
            const data = await invoke<any>("get_config");
            updater(data);
            await invoke("update_config", { newConfig: data });
            return data;
        } catch {
            toast("設定の更新に失敗しました");
            return null;
        }
    };

    const handleLearningChange = async (learning: boolean) => {
        const data = await updateConfig((data) => {
            data.learning = data.learning ?? {};
            data.learning.enable = learning;
        });
        if (data) {
            setValue((prev) => ({ ...prev, learning }));
        }
    };

    const handleLiveConversionChange = async (live_conversion: boolean) => {
        const data = await updateConfig((data) => {
            data.conversion = data.conversion ?? {};
            data.conversion.live_conversion = live_conversion;
        });
        if (data) {
            setValue((prev) => ({ ...prev, live_conversion }));
        }
    };

    const handlePredictionChange = async (prediction: boolean) => {
        const data = await updateConfig((data) => {
            data.conversion = data.conversion ?? {};
            data.conversion.prediction = prediction;
        });
        if (data) {
            setValue((prev) => ({ ...prev, prediction }));
        }
    };

    const handleTypoCorrectionChange = async (typo_correction: boolean) => {
        const data = await updateConfig((data) => {
            data.conversion = data.conversion ?? {};
            data.conversion.typo_correction = typo_correction;
        });
        if (data) {
            setValue((prev) => ({ ...prev, typo_correction }));
        }
    };

    const handleInputStyleChange = async (input_style: string) => {
        const data = await updateConfig((data) => {
            data.conversion = data.conversion ?? {};
            data.conversion.input_style = input_style;
        });
        if (data) {
            setValue((prev) => ({ ...prev, input_style }));
        }
    };

    const handleCustomInputTablePathChange = async (custom_input_table_path: string) => {
        setValue((prev) => ({ ...prev, custom_input_table_path }));
        await updateConfig((data) => {
            data.conversion = data.conversion ?? {};
            data.conversion.custom_input_table_path = custom_input_table_path;
        });
    };

    const handleCandidateNumberSelectionChange = async (candidate_number_selection: boolean) => {
        const data = await updateConfig((data) => {
            data.conversion = data.conversion ?? {};
            data.conversion.candidate_number_selection = candidate_number_selection;
        });
        if (data) {
            setValue((prev) => ({ ...prev, candidate_number_selection }));
        }
    };

    const handleSymbolInputStyleChange = async (symbol_input_style: string) => {
        const data = await updateConfig((data) => {
            data.conversion = data.conversion ?? {};
            data.conversion.symbol_input_style = symbol_input_style;
        });
        if (data) {
            setValue((prev) => ({ ...prev, symbol_input_style }));
        }
    };

    const handleClearLearning = async () => {
        try {
            await invoke("clear_learning_data");
            toast("学習データを削除しました");
        } catch {
            toast("学習データの削除に失敗しました");
        }
    };

    return (
        <div className="space-y-8">
            <section className="space-y-2">
                <h1 className="text-sm font-bold text-foreground">変換</h1>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <WandSparkles />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            ライブ変換
                        </p>
                        <p className="text-xs text-muted-foreground">
                            入力中にシステム辞書から候補を表示します
                        </p>
                    </div>
                    <Switch checked={value.live_conversion} onCheckedChange={handleLiveConversionChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Lightbulb />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            予測変換
                        </p>
                        <p className="text-xs text-muted-foreground">
                            入力途中の読みから候補を先読みします
                        </p>
                    </div>
                    <Switch checked={value.prediction} onCheckedChange={handlePredictionChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <SpellCheck />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            typo correction
                        </p>
                        <p className="text-xs text-muted-foreground">
                            1文字違いの読みから候補を補正します
                        </p>
                    </div>
                    <Switch checked={value.typo_correction} onCheckedChange={handleTypoCorrectionChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Keyboard />
                    <div className="flex-1 space-y-3">
                        <div className="space-y-1">
                            <p className="text-sm font-medium leading-none">
                                入力テーブル
                            </p>
                            <p className="text-xs text-muted-foreground">
                                標準ローマ字、AZIK、カスタムTSVを切り替えます
                            </p>
                        </div>
                        <div className="flex flex-col gap-2 sm:flex-row">
                            <Select value={value.input_style} onValueChange={handleInputStyleChange}>
                                <SelectTrigger className="w-40">
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="default">標準</SelectItem>
                                    <SelectItem value="azik">AZIK</SelectItem>
                                    <SelectItem value="custom">カスタム</SelectItem>
                                </SelectContent>
                            </Select>
                            <Input
                                value={value.custom_input_table_path}
                                disabled={value.input_style !== "custom"}
                                placeholder="%APPDATA%\\Azookey\\input_table.tsv"
                                onChange={(event) => handleCustomInputTablePathChange(event.target.value)}
                            />
                        </div>
                    </div>
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <BrainCircuit />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            学習
                        </p>
                        <p className="text-xs text-muted-foreground">
                            確定した候補を次回以降の変換に反映します
                        </p>
                    </div>
                    <Switch checked={value.learning} onCheckedChange={handleLearningChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Keyboard />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            数字キーで候補選択
                        </p>
                        <p className="text-xs text-muted-foreground">
                            候補表示中に1-9/0で候補を直接選択します
                        </p>
                    </div>
                    <Switch checked={value.candidate_number_selection} onCheckedChange={handleCandidateNumberSelectionChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Keyboard />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            記号入力
                        </p>
                        <p className="text-xs text-muted-foreground">
                            かな入力中の句読点と記号の扱いを選択します
                        </p>
                    </div>
                    <Select value={value.symbol_input_style} onValueChange={handleSymbolInputStyleChange}>
                        <SelectTrigger className="w-40">
                            <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                            <SelectItem value="japanese">日本語記号</SelectItem>
                            <SelectItem value="raw">そのまま</SelectItem>
                        </SelectContent>
                    </Select>
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Trash2 />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            学習データを削除
                        </p>
                        <p className="text-xs text-muted-foreground">
                            これまで確定した候補の学習結果をリセットします
                        </p>
                    </div>
                    <Button variant="secondary" onClick={handleClearLearning}>
                        削除
                    </Button>
                </div>
            </section>
            <section className="space-y-2">
                <h1 className="text-sm font-bold text-foreground">バージョンと更新プログラム</h1>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <RefreshCcw />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            v0.1.0-alpha.1
                        </p>
                    </div>
                    <Button  variant="secondary">
                        <a href="https://github.com/fkunn1326/azooKey-Windows/releases" className="flex items-center gap-x-2" target="_blank" rel="noopener noreferrer">
                            <ExternalLink />
                            更新を確認する
                        </a>
                    </Button>
                </div>
            </section>
            {/* <section className="space-y-2">
                <h1 className="text-sm font-bold text-foreground">診断とフィードバック</h1>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <FileChartColumn />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            診断データ
                        </p>
                        <p className="text-xs text-muted-foreground">
                            診断データを保存し、バグの修正に役立てます
                        </p>
                    </div>
                    <Switch />
                </div>
            </section> */}
        </div>
    )
}

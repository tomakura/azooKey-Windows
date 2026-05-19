import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { invoke } from "@tauri-apps/api/core";
import { BrainCircuit, ExternalLink, Lightbulb, RefreshCcw, Trash2, WandSparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

export const General = () => {
    const [value, setValue] = useState({
        learning: true,
        live_conversion: true,
        prediction: true,
    });

    useEffect(() => {
        invoke<any>("get_config")
            .then((data) => {
                setValue({
                    learning: data.learning?.enable ?? true,
                    live_conversion: data.conversion?.live_conversion ?? true,
                    prediction: data.conversion?.prediction ?? true,
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

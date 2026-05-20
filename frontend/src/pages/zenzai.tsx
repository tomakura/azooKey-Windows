import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Bot, User, Cpu, FileCode2, Gauge, SquareTerminal, WandSparkles } from "lucide-react";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import { useEffect, useState } from "react";
import { toast } from "sonner"
import { invoke } from '@tauri-apps/api/core';
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"

const ToolTipSelectItem = ({
    name,
    value,
    disabled,
    tooltip
}: {
    name: string;
    value: string;
    disabled: boolean;
    tooltip: string;
}) => {
    return (
        <TooltipProvider>
            <Tooltip>
                <TooltipTrigger>
                    <SelectItem value={value} disabled={disabled}>
                        {name}
                    </SelectItem>
                </TooltipTrigger>
                {disabled && <TooltipContent side="left">
                    {tooltip}
                </TooltipContent>}
            </Tooltip>
        </TooltipProvider>
    )
}

export const Zenzai = () => {
    const [value, setValue] = useState({
        enable: false,
        profile: "",
        backend: "",
        inference_limit: 1,
        timeout_ms: 1500,
        model_path: "",
        command_path: "",
    });
    const [magicConversion, setMagicConversion] = useState({
        enable: false,
        command_path: "",
        timeout_ms: 1500,
    });

    const [capability, setCapability] = useState({
        cpu: true,
        cuda: false,
        vulkan: false,
    });

    // Load config on component mount
    useEffect(() => {
        invoke<any>("get_config")
            .then((data) => {
                const zenzai = data.zenzai;
                setValue({
                    enable: zenzai.enable,
                    profile: zenzai.profile,
                    backend: zenzai.backend,
                    inference_limit: zenzai.inference_limit ?? 1,
                    timeout_ms: zenzai.timeout_ms ?? 1500,
                    model_path: zenzai.model_path ?? "",
                    command_path: zenzai.command_path ?? "",
                });
                const magic = data.magic_conversion ?? {};
                setMagicConversion({
                    enable: magic.enable ?? false,
                    command_path: magic.command_path ?? "",
                    timeout_ms: magic.timeout_ms ?? 1500,
                });
            })
            .catch(() => {
                // Keep default values if config fetch fails
            });

        invoke("check_capability").then((capability: any) => {
            setCapability({
                cpu: capability["cpu"],
                cuda: capability["cuda"],
                vulkan: capability["vulkan"],
            });
        })
    }, []);

    const updateConfig = async (updater: (config: any) => void) => {
        try {
            const data = await invoke<any>("get_config");
            updater(data);
            await invoke("update_config", { newConfig: data });
            return data;
        } catch (error) {
            toast("設定の更新に失敗しました");
            return null;
        }
    };

    const handleZenzaiChange = async () => {
        const data = await updateConfig((data) => {
            data.zenzai.enable = !value.enable;
        });
        
        if (data) {
            setValue((prev) => ({ ...prev, enable: data.zenzai.enable }));
        }
    };

    const handleProfileChange = (event: React.ChangeEvent<HTMLTextAreaElement>) => {
        const newProfile = event.target.value;
        setValue((prev) => ({ ...prev, profile: newProfile }));
        
        updateConfig((data) => {
            data.zenzai.profile = newProfile;
        });
    };

    const handleBackendChange = async (backend: string) => {
        const data = await updateConfig((data) => {
            data.zenzai.backend = backend;
        });
        
        if (data) {
            setValue((prev) => ({ ...prev, backend }));
            toast("バックエンドが変更されました", {
                description: "変更を適用するには、IMEサーバーを再起動してください",
                duration: 10000,
            });
        }
    };

    const handleModelPathChange = (event: React.ChangeEvent<HTMLInputElement>) => {
        const model_path = event.target.value;
        setValue((prev) => ({ ...prev, model_path }));

        updateConfig((data) => {
            data.zenzai.model_path = model_path;
        });
    };

    const handleCommandPathChange = (event: React.ChangeEvent<HTMLInputElement>) => {
        const command_path = event.target.value;
        setValue((prev) => ({ ...prev, command_path }));

        updateConfig((data) => {
            data.zenzai.command_path = command_path;
        });
    };

    const handleInferenceLimitChange = (event: React.ChangeEvent<HTMLInputElement>) => {
        const inference_limit = Math.max(1, Number(event.target.value) || 1);
        setValue((prev) => ({ ...prev, inference_limit }));

        updateConfig((data) => {
            data.zenzai.inference_limit = inference_limit;
        });
    };

    const handleTimeoutChange = (event: React.ChangeEvent<HTMLInputElement>) => {
        const timeout_ms = Math.max(100, Number(event.target.value) || 1500);
        setValue((prev) => ({ ...prev, timeout_ms }));

        updateConfig((data) => {
            data.zenzai.timeout_ms = timeout_ms;
        });
    };

    const handleMagicConversionChange = async () => {
        const data = await updateConfig((data) => {
            data.magic_conversion = data.magic_conversion ?? {};
            data.magic_conversion.enable = !magicConversion.enable;
        });

        if (data) {
            setMagicConversion((prev) => ({ ...prev, enable: data.magic_conversion.enable }));
        }
    };

    const handleMagicCommandPathChange = (event: React.ChangeEvent<HTMLInputElement>) => {
        const command_path = event.target.value;
        setMagicConversion((prev) => ({ ...prev, command_path }));

        updateConfig((data) => {
            data.magic_conversion = data.magic_conversion ?? {};
            data.magic_conversion.command_path = command_path;
        });
    };

    const handleMagicTimeoutChange = (event: React.ChangeEvent<HTMLInputElement>) => {
        const timeout_ms = Math.max(100, Number(event.target.value) || 1500);
        setMagicConversion((prev) => ({ ...prev, timeout_ms }));

        updateConfig((data) => {
            data.magic_conversion = data.magic_conversion ?? {};
            data.magic_conversion.timeout_ms = timeout_ms;
        });
    };

    return (
        <div className="space-y-8">
            <section className="space-y-2">
                <h1 className="text-sm font-bold text-foreground">Zenzai</h1>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Bot />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            Zenzaiを有効化
                        </p>
                        <p className="text-xs text-muted-foreground">
                            Zenzaiを有効にして、変換精度を向上させます
                        </p>
                    </div>
                    <Switch checked={value.enable} onCheckedChange={handleZenzaiChange} />
                </div>
                <div className="space-y-4 rounded-md border p-4">
                    <div className="flex items-center space-x-4 ">
                        <User />
                        <div className="flex-1 space-y-1">
                            <p className="text-sm font-medium leading-none">
                                変換プロファイル
                            </p>
                            <p className="text-xs text-muted-foreground">
                                Zenzaiで利用されるユーザー情報を設定します
                            </p>
                        </div>
                    </div>
                    <Textarea placeholder="例）山田太郎、数学科の学生。" value={value.profile} disabled={!value.enable} onChange={handleProfileChange} />
                </div>
                <div className="space-y-4 rounded-md border p-4">
                    <div className="flex items-center space-x-4">
                        <FileCode2 />
                        <div className="flex-1 space-y-1">
                            <p className="text-sm font-medium leading-none">
                                モデル
                            </p>
                            <p className="text-xs text-muted-foreground">
                                空欄の場合はIMEサーバーと同じフォルダーのzenz.ggufを使用します
                            </p>
                        </div>
                    </div>
                    <Input placeholder="C:\\path\\to\\zenz.gguf" value={value.model_path} disabled={!value.enable} onChange={handleModelPathChange} />
                </div>
                <div className="space-y-4 rounded-md border p-4">
                    <div className="flex items-center space-x-4">
                        <SquareTerminal />
                        <div className="flex-1 space-y-1">
                            <p className="text-sm font-medium leading-none">
                                llama.cpp CLI
                            </p>
                            <p className="text-xs text-muted-foreground">
                                空欄の場合は選択中のバックエンドからPATH経由でllama-cli.exeを使用します
                            </p>
                        </div>
                    </div>
                    <Input placeholder="C:\\path\\to\\llama-cli.exe" value={value.command_path} disabled={!value.enable} onChange={handleCommandPathChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Gauge />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            推論トークン数
                        </p>
                        <p className="text-xs text-muted-foreground">
                            候補番号を出力するための最大トークン数
                        </p>
                    </div>
                    <Input className="w-24" type="number" min={1} max={8} value={value.inference_limit} disabled={!value.enable} onChange={handleInferenceLimitChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Gauge />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            タイムアウト
                        </p>
                        <p className="text-xs text-muted-foreground">
                            Zenzaiの応答を待つ最大時間
                        </p>
                    </div>
                    <Input className="w-28" type="number" min={100} step={100} value={value.timeout_ms} disabled={!value.enable} onChange={handleTimeoutChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Cpu />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            バックエンド
                        </p>
                        <p className="text-xs text-muted-foreground">
                            Zenzaiを利用するバックエンドを選択します
                        </p>
                    </div>
                    <Select disabled={!value.enable} value={value.backend} onValueChange={handleBackendChange}>
                        <SelectTrigger className="w-48">
                            <SelectValue placeholder="バックエンドを選択" />
                        </SelectTrigger>
                        <SelectContent className="flex flex-col">
                            <ToolTipSelectItem name="CPU (非推奨)" value="cpu" disabled={!capability.cpu} tooltip="" />
                            <ToolTipSelectItem name="CUDA (NVIDIA GPU)" value="cuda" disabled={!capability.cuda} tooltip="CUDA Toolkit 12をインストールする必要があります" />
                            <ToolTipSelectItem name="Vulkan" value="vulkan" disabled={!capability.vulkan} tooltip="お使いのPCはVulkanに対応していません" />
                        </SelectContent>
                    </Select>
                </div>
            </section>
            <section className="space-y-2">
                <h1 className="text-sm font-bold text-foreground">いい感じ変換</h1>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <WandSparkles />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            いい感じ変換を有効化
                        </p>
                        <p className="text-xs text-muted-foreground">
                            読みと文脈を外部コマンドに渡し、返された各行を候補に追加します
                        </p>
                    </div>
                    <Switch checked={magicConversion.enable} onCheckedChange={handleMagicConversionChange} />
                </div>
                <div className="space-y-4 rounded-md border p-4">
                    <div className="flex items-center space-x-4">
                        <SquareTerminal />
                        <div className="flex-1 space-y-1">
                            <p className="text-sm font-medium leading-none">
                                変換コマンド
                            </p>
                            <p className="text-xs text-muted-foreground">
                                --reading と --context を受け取り、候補を1行ずつ標準出力へ返す実行ファイル
                            </p>
                        </div>
                    </div>
                    <Input placeholder="C:\\path\\to\\magic-conversion.exe" value={magicConversion.command_path} disabled={!magicConversion.enable} onChange={handleMagicCommandPathChange} />
                </div>
                <div className="flex items-center space-x-4 rounded-md border p-4">
                    <Gauge />
                    <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium leading-none">
                            タイムアウト
                        </p>
                        <p className="text-xs text-muted-foreground">
                            いい感じ変換の応答を待つ最大時間
                        </p>
                    </div>
                    <Input className="w-28" type="number" min={100} step={100} value={magicConversion.timeout_ms} disabled={!magicConversion.enable} onChange={handleMagicTimeoutChange} />
                </div>
            </section>
        </div>
    )
}

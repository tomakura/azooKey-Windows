import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";

export async function saveConfig(config: unknown) {
    const applied = await invoke<boolean>("update_config", { newConfig: config });
    if (!applied) toast("設定を保存しました。変換エンジンの次回起動時に反映されます");
}

let pending: Promise<unknown> = Promise.resolve();

// Read and save together so consecutive setting changes cannot overwrite each other.
export function changeConfig(updater: (config: any) => void) {
    const result = pending.then(async () => {
        const config = await invoke<any>("get_config");
        updater(config);
        await saveConfig(config);
        return config;
    });
    pending = result.then(() => undefined, () => undefined);
    return result;
}

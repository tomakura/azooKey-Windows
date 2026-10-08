# 配布版の検証記録

対象: 0.1.0-alpha.2。READMEのチェックは実装済みを表し、Windowsへの登録後の受け入れ検証とは区別します。

## 実行済み

- Rust: shared、converter、IMEクライアントの27テスト。
- Swift: 実辞書と品詞・変換オプションのテスト。
- Rustと実Swiftエンジンの統合: 漢字変換、登録語の優先、予測の切り替え、学習の保存・無効化・削除、AZIKとカスタム入力、削除・カーソル・部分確定、繰り返し変換とFFI解放、候補数制限。
- いい感じ変換: 実子プロセスへ読みと文脈を渡し、正常応答、非ゼロ終了、空の出力、タイムアウトを確認。
- Zenzai: 実モデルをCPU、CUDA、Vulkanそれぞれで読み込み、文章を変換。パーソナライズのファイル読み込みとエラーも確認。
- 設定画面: 配布版Tauri/WebView2をPlaywrightで操作。CSVファイル取り込み、保存、実サーバーへの反映、登録語が先頭候補になること、TSVファイル書き出し、色の保存・連続変更・再読み込み後のプレビュー、不正な入力テーブルの拒否、カスタム入力の反映、学習削除、ライブ変換・予測の切り替え、Zenzai・パーソナライズ・外部プロバイダーから実処理までを確認。Tauriやサーバーのモックは使用していません。
- `verify-safe.ps1`: fmt、Clippy、x64/x86チェック、フロントエンドの4テストとproductionビルド、配布設定の静的チェック。
- x64リリース、x86 IME DLL、Swiftリリース、埋め込みフロントエンドをビルド。
- 配布フォルダー内のSwift DLL・リソース・辞書・モデルを使い、実エンジン統合テストを実行。
- 配布全3,471ファイルのSHA256・サイズ・収録数、x64/x86のPEアーキテクチャを検証。
- Inno Setupインストーラーをコンパイル。

テストの設定・辞書は`target/verification`内の専用APPDATAへ保存し、`AZOOKEY_INSTANCE=release_verification`の専用パイプを使用しています。既存のIME設定は変更していません。

## 再検証

```powershell
powershell -ExecutionPolicy Bypass -File scripts/verify-safe.ps1
powershell -ExecutionPolicy Bypass -File scripts/verify-release.ps1
cargo build -p azookey-converter --example provider_fixture
# 実エンジンのテストにはSwiftと選択したllamaバックエンドのDLLがPATHに必要です。
$env:Path = "$PWD/build/release;$PWD/build/release/llama_cpu;$env:Path"
$env:AZOOKEY_TEST_RESOURCES = "$PWD/build/release"
$env:AZOOKEY_TEST_ZENZAI = '1'
cargo test -p azookey-server --bin azookey-server -- --nocapture
```

`llama_cpu`を`llama_cuda`または`llama_vulkan`へ変えると他のバックエンドを検証できます。実行ログと画面キャプチャは`target/verification`にあります。

## 残る受け入れ検証

2026-10-08に、ユーザー承認を受けてこのPCの既存IMEを更新しました。設定・辞書・学習データは先にバックアップしました。

- インストール終了コード0、Windowsの再起動不要。
- インストールされた3,471ファイルをSHA256で照合し、配布物と一致。
- x64/x86のCOM登録、日本語IMEプロファイル登録、DLLの配置先とApartment設定を実レジストリで確認。
- 起動タスクの実行結果0と、launcher・変換エンジン・UIAccess付き候補UIの実プロセスを確認。
- アンインストール終了コード0、x64/x86のCOM登録と起動タスクの解除、既存設定の保持を確認。
- 続けて再インストールし、終了コード0、両アーキテクチャの登録と起動タスクの復元、設定保持を確認。更新版がインストールされた状態に戻しています。
- インストール済み候補UIを専用APPDATA・専用パイプで別起動し、候補・選択・位置・入力モード・表示の実IPC応答を確認。再実行用クライアントは`crates/server/examples/window_probe.rs`です。

Windows操作ヘルパーは未接続です。UIAccess付き候補UIのWebView2デバッグポートにも接続できなかったため、実画面の描画やTSF実入力を確認済みとはしていません。隔離したテストUIは終了し、通常のIMEを残しました。

インストール後に次を確認する必要があります。

1. x64アプリとx86アプリでIMEを選択し、入力・候補表示・確定・部分確定・削除・モード切り替えを行う。
2. ライブ変換の有効・無効、Space/Tab、数字キー選択、F6〜F10、入力位置への候補追従を確認する。
3. 候補ウィンドウと入力モード表示の色・カスタムCSSが更新されることを確認する。
4. 学習、登録語、予測、外部プロバイダー、Zenzaiの設定が実入力に反映されることを確認する。
5. サインインし直した際の自動起動を確認する。タスクからの手動実行とアンインストール時の登録解除は確認済み。

この確認が終わるまでは、配布後の動作を含めた完成率を100%とはしていません。

## 生成した配布物

- インストーラー: `build/azookey-setup.exe` (510889502 bytes)
- SHA256: `D61EE2C3F8D7DBE4B3CC40F2CDDC4579C348D5C72D44B3BC8A3D54F09A2A28AB`
- ハッシュ一覧: `build/release/manifest.json`、照合用: `build/azookey-setup.exe.sha256`

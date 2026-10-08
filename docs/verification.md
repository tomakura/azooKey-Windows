# 配布版の検証記録

対象: 0.1.0-alpha.3。READMEのチェックは実装済みを表し、Windowsへの登録後の受け入れ検証とは区別します。

## 実行済み

- Rust: shared、converter、IMEクライアントの30テスト。
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

テストの設定・辞書は`target/verification`内の専用APPDATAへ保存し、`AZOOKEY_INSTANCE=release_verification`の専用パイプを使用しています。隔離テストとは別に、ユーザー指定に従い実使用設定のライブ変換をオフにしました。予測はオンのままです。変更前の設定もバックアップしています。

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

以下のインストール・解除記録はalpha.2時点です。alpha.3も2026-10-08に更新済みです。更新時の検証結果は後述します。

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
2. ライブ変換の有効・無効、Spaceの通常変換とTabの予測選択、予測が自動で本文に入らないこと、ひらがなキーを繰り返しても日本語入力を維持すること、数字キー選択、F6〜F10、入力位置への候補追従を確認する。
3. 候補ウィンドウと入力モード表示の色・カスタムCSSが更新されることを確認する。
4. 学習、登録語、予測、外部プロバイダー、Zenzaiの設定が実入力に反映されることを確認する。
5. サインインし直した際の自動起動を確認する。タスクからの手動実行とアンインストール時の登録解除は確認済み。

この確認が終わるまでは、配布後の動作を含めた完成率を100%とはしていません。

## alpha.3の入力操作と設定画面

ユーザーから実入力ができるとの回答を得た一方、ライブ変換・予測の自動挿入、ひらがなキー、設定画面への入口に問題が報告されました。

- ライブ変換の初期値をオフにし、設定からオンにできる機能として保持。
- Swiftの予測をmanualMixで分離し、通常候補と予測候補の種別をFFI・IPCで伝達。ライブ表示は通常候補のみ、Spaceは通常変換、Tabは予測候補選択、Enterで確定。
- 文節確定後の追加入力でも、ライブ変換オフ時に本文へ予測が先に挿入されないよう修正。
- VK_KANA・VK_IME_ON・VK_DBE_HIRAGANAは日本語入力をオンにし、同じキーを繰り返してもオフにしない。VK_IME_OFFで英数へ切り替え。
- Windows言語バーの右クリックが入力モードを切り替えていた原因を修正し、設定画面を開く。浮動表示には歯車、スタートメニューには「Azookey 設定」を追加。
- 初回更新では使用中の旧IME DLLが上書きを拒否したため、インストーラーを版ごとのDLL名へ変更。既存アプリが読み込んだ旧DLLを保持し、新しく開くアプリには更新版を登録する。使用中DLLのアンインストール時は削除を再起動後へ延期できるようにした。
- 続いて旧IMEが読み込んだ`vcruntime140.dll`の置換で停止した。既存ファイルと配布ファイルのSHA256が一致することを確認し、VCランタイムを強制上書き対象から分離した。Inno Setupの`replacesameversion`で、同一バージョン・同一内容を保持し、同じバージョンでも内容が異なる場合は更新する（[公式仕様](https://jrsoftware.org/ishelp/topic_filessection.htm)）。
- 設定画面の「全般」にエンジン再起動を追加。設定保存後にサーバーへ終了を依頼し、選択したバックエンドで新しいプロセスを起動する。既存IPC接続から異なるPIDの応答を得てから完了と表示する。
- 実配布版の設定画面をPlaywrightで操作し、ライブ変換の初期値オフ・予測オン、ライブ変換のオン／オフ保存と再読み込みを確認。
- 最終バイナリでもalpha.3の版表示、再起動ボタン、ライブ変換オフ・予測オンを再確認（`gui-alpha3-final-result.json`、`gui-alpha3-final.png`）。
- 再起動ボタンを3回操作し、PIDが49384→15760→50976→41132へ変わること、既存接続の再接続、再起動後の漢字変換、実際に読み込まれたllama.dllがCPU→Vulkan→CPUへ変わることを確認。画面の完了表示も確認。
- 今回の証跡: `safe-input-ux.log`、`swift-test-input-ux.log`、`engine-input-ux-package.log`、`gui-input-ux.log`、`gui-input-ux-result.json`、`restart-input-ux-result.json`、`gui-input-ux.png`（すべて`target/verification`内）。

### alpha.3の実更新

- インストール終了コード0、Windowsの再起動不要。使用中の旧IMEと同一内容のVCランタイムを保持して更新できた。
- インストールされた全3,471ファイルのSHA256を配布物と照合し、一致。x64/x86のCOM登録がそれぞれ`azookey-0.1.0-alpha.3.dll`、`azookey32-0.1.0-alpha.3.dll`を指し、日本語IMEプロファイルが存在することを確認。
- 既存設定の内容が保持され、ライブ変換オフ・予測オンを確認。
- スタートメニューの「Azookey 設定」の配置とリンク先を確認。
- launcher・候補UI・変換エンジンの起動を確認。入力を変更しない`query --status`でインストール済みエンジンのPID 5072から応答を得た。
- 証跡: `upgrade-input-ux-result.json`、`upgrade-input-ux.log`、`installer-alpha3-artifact.json`。更新後の実キー操作はユーザー確認待ち。起動中だったアプリは保存して開き直すと新しいIME DLLを読み込む。

## 生成した配布物

- インストーラー: `build/azookey-setup.exe` (510960939 bytes)
- SHA256: `9B159FB438124CC108359FB9CD1C5E3628DE067262321BAA00C7FFD70F3446A2`
- ハッシュ一覧: `build/release/manifest.json`、照合用: `build/azookey-setup.exe.sha256`

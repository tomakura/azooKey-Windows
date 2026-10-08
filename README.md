# azooKey for Windows

[AzooKeyKanaKanjiConverter](https://github.com/azooKey/AzooKeyKanaKanjiConverter)を利用したWindows版IMEです。

> [!WARNING]
> 現在開発中であるため、安定性や機能に関しては保証できません。使用する際は自己責任でお願いします。

# インストール方法
[Release](https://github.com/fkunn1326/azooKey-Windows/releases)から`azookey-setup.exe`をダウンロードし、インストーラーを実行してください。

# 機能

- [x] ライブ変換
- [x] Zenzaiを使用したニューラルかな漢字変換

- [x] 学習機能
- [x] 辞書登録機能
- [x] テーマ変更機能
- [x] 辞書のインポート/エクスポート機能
- [x] いい感じ変換
- [x] 個人最適化システム
- [x] 予測変換

チェックは機能の実装状況です。配布版の検証記録と、インストール後に必要な実入力の確認は[検証記録](docs/verification.md)を参照してください。

# 設定

スタートメニューの「Azookey 設定」、入力モード表示の歯車ボタン、またはWindows言語バーの「あ／A」の右クリックで開けます。実行ファイルは `%APPDATA%\Azookey\azookey_settings.exe` です。

## Zenzai

### 変換プロファイル
設定で変換プロファイルを指定すると、プロファイルに応じた変換候補が表示されます。

### バックエンド
以下の3種類のバックエンドをサポートしています。

- **CPU**: GPUがない環境でも利用できます。GPU版より推論に時間がかかります。
- **CUDA**: NVIDIAのGPUと対応するドライバーが必要です。配布版にはバックエンドのDLLを同梱します。
- **Vulkan**: Vulkanに対応するGPUとドライバーが必要です。

バックエンドの変更後は、設定画面の「全般」から「変換エンジンを再起動」を押してください。モデル、辞書、実行ファイルが不足している場合はエラーを表示します。

### 個人最適化

UTF-8のテキストファイルを指定し、個人最適化を有効にすると、内容を変換プロファイルへ追加します。読み込む上限は4,096文字です。Zenzaiを有効にして使用してください。

## 学習・予測・ライブ変換

確定した候補を学習し、次回以降の変換に利用します。設定画面で学習の有効・無効と学習データの削除を操作できます。ライブ変換は初期状態でオフです。入力中はかなを表示し、Spaceで通常の変換候補を選びます。Tabで予測候補を選び、Enterで確定します。ライブ変換をオンにしても、予測候補は自動で本文に挿入しません。ひらがなキーは日本語入力をオンにします。

## ユーザー辞書

読み、候補、品詞を登録し、保存すると変換エンジンへ反映されます。UTF-8のTSVまたはCSVを取り込み、TSVを書き出せます。形式は `読み<TAB>候補<TAB>品詞` または `読み,候補,品詞` です。CSVの引用符とBOMに対応します。取り込み後は「保存」を押してください。設定と辞書は `%APPDATA%\Azookey` に保存されます。

## テーマ

「外観」で背景色、アクセントカラー、文字色またはカスタムCSSを設定できます。実際の候補表示と入力モード表示には次の表示更新時に反映されます。

## いい感じ変換

外部の変換プロバイダーを指定します。実行ファイルへ `--reading <読み> --context <直前の文章>` を渡し、標準出力の各行を候補として読み込みます。実行失敗、空の出力、タイムアウトはエラーとして返します。プロバイダー自体は同梱していません。

# コミュニティ

## 開発を支援する
- [GitHub Sponsors (Miwa)](https://github.com/sponsors/ensan-hcl): 変換エンジンの開発者
- [Patreon (fkunn1326)](https://www.patreon.com/c/fkunn1326): Windowsに移植した人

## 開発に参加する

### 開発環境のセットアップ

- [Rust](https://www.rust-lang.org/tools/install)
- [Swift for Windows](https://www.swift.org/install/windows/) (Swift 6.3.2で検証)
- [protoc](https://protobuf.dev/installation/) 
- [node.js](https://nodejs.org/en/download/)
- [inno setup](https://jrsoftware.org/isinfo.php)

### ビルド

#### リポジトリのクローン
```
git clone https://github.com/fkunn1326/azookey-Windows --recursive
```
`--recursive`オプションを付けて、サブモジュールも一緒にクローンしてください。

#### cargo-makeのインストール
```
cargo install --force cargo-make
```

#### ビルド
```
cargo make build --release
```
配布版は`--release`でビルドしてください。CPU、CUDA、Vulkan用のazooKey/llama.cpp b4846 DLLをそれぞれ`llama_cpu`、`llama_cuda`、`llama_vulkan`へ配置し、CPU版の`llama.lib`を`server-swift`へ配置します。`zenz.gguf`もリポジトリ直下へ配置してください。取得元と手順は`.github/workflows/actions.yml`を参照してください。

`build/release`に実行ファイル、辞書、モデル、Swiftランタイムとハッシュ一覧が格納され、`build/azookey-setup.exe`が生成されます。`powershell -ExecutionPolicy Bypass -File scripts/verify-safe.ps1`で安全な検証を実行できます。

`launcher.exe`を管理者権限で実行すると、azookeyの変換エンジンが起動します。

また、IMEを登録する際は以下のように`regsvr32.exe`を使用して登録する必要があります。
```powershell
& "$env:SystemRoot\System32\regsvr32.exe" "path/to/build/release/azookey_windows.dll" /s
& "$env:SystemRoot\SysWOW64\regsvr32.exe" "path/to/build/release/x86/azookey_windows.dll" /s
```
逆に登録を解除する場合は`/u`オプションを付けて実行してください。

#### 開発時のヒント
- 開発は仮想マシンまたは専用のPCで行うことを推奨します。IMEがクラッシュするとWindowsがフリーズする可能性があります。
- IMEを解除する際、IMEを使用中のアプリケーション（メモ帳など）を終了しないと、解除できないことがあります。

# 関連

- [azooKey/azooKey](https://github.com/azooKey/azooKey): iOS / iPadOS向けの日本語キーボードアプリ
- [7ka-Hiira/fcitx5-hazkey](https://github.com/7ka-Hiira/fcitx5-hazkey): fcitx5向けのLinux版azooKey
- [azooKey/AzookeyKanakanjiConverter](https://github.com/azooKey/AzooKeyKanaKanjiConverter): azooKeyの変換エンジン

# 参考
本プロジェクトの開発にあたり、以下のリソースを参考にしました。ありがとうございます！
- [OMAMA-Taioan/khiin-rs](https://github.com/OMAMA-Taioan/khiin-rs/tree/master/windows)
- [google/mozc](https://github.com/google/mozc/tree/master/src/win32/tip)
- [microsoft/Windows-classic-samples](https://github.com/microsoft/Windows-classic-samples/tree/main/Samples/Win7Samples/winui/input/tsf/textservice)
- [dec32/ajemi](https://github.com/dec32/ajemi)
- https://zenn.dev/mkpoli/scraps/6dc57fcd0335cf

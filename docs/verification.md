# 配布版の検証記録

対象: 0.1.0-alpha.5。READMEのチェックは実装済みを表し、Windowsへの登録後の受け入れ検証とは区別します。

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
- 初回更新では使用中の旧IME DLLが上書きを拒否したため、インストーラーを版ごとのDLL名へ変更。既存アプリが読み込んだ旧DLLを保持し、更新版を登録する。使用中DLLのアンインストール時は削除を再起動後へ延期できるようにした。
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
- 証跡: `upgrade-input-ux-result.json`、`upgrade-input-ux.log`、`installer-alpha3-artifact.json`。この時点では、起動中アプリやExplorerが更新版DLLを読み込んだことまでは確認できていなかった。

## alpha.4の右クリックと旧DLLの残留

- alpha.3更新後も右クリックが入力モード切り替えになり、かなキーが効かないとの報告を受けた。
- 実プロセスのモジュールを調べ、Notepad (PID 49028)、Explorer (15832)、Code、Discord、SearchHost、Vivaldiが旧`azookey.dll`を読み込んでいることを確認。ユーザーに有効なCOM登録はalpha.3を指し、HKCUの別登録も存在しなかった。登録先の更新と、実際に使われているDLLは一致していなかった。
- 作業を保存してWindowsからサインアウト・再サインインする必要がある旨をインストーラー完了画面に追加。アプリの強制終了や自動サインアウトは行わない。
- 新版の右クリックも設定画面を直接起動していたため、「設定を開く」を選べるネイティブメニューへ変更。キャンセル時は入力モードを変更しない。[OnClickの公式仕様](https://learn.microsoft.com/ja-jp/windows/win32/api/ctfutb/nf-ctfutb-itflangbaritembutton-onclick)のクリック種別と画面座標を利用する。
- クライアントの既存テスト7件、clippy、フォーマット検査、x64/x86の配布用ビルド、設定画面のビルド、インストーラー静的検証を実施。
- かなキーの追加変更はせず、alpha.3で追加済みの日本語入力オン処理を保持。サインインし直した後の実キー操作と右クリックメニュー表示は未確認。
- alpha.4の実更新は終了コード0で完了。全3,471ファイルのSHA256、x64/x86のCOM登録、日本語IMEプロファイル、設定ショートカットを照合。既存設定を保持し、ライブ変換オフ・予測オンを確認した。
- 証跡: `loaded-ime-before-alpha4.json`、`upgrade-alpha4-result.json`、`upgrade-alpha4.log`、`installer-alpha4-artifact.json`（`target/verification`内）。インストーラー上の再起動要求はなくても、旧IMEを読み込んだプロセスを切り替えるにはサインアウトが必要。

## alpha.5の入力遅延・予測表示・英字入力

- alpha.4でサインインし直した後、右クリックメニューとかなキーの両方が直ったとのユーザー確認を得た。一方、ライブ変換オフでも長文入力が遅く、通常候補が常時表示され、「Windows」がかな混じりになる問題が報告された。
- ライブ変換オフでもAppendText・RemoveText・ShrinkTextが毎回候補生成を行い、入力クライアントが同期的に待っていた。文脈更新も同じ推論ロックを待っていた。
- 本家SwiftのComposingTextを入力専用の状態とロックへ分離。かな・英字の編集RPCは推論ロックを取得せず、候補を返さない。文脈はクライアントで保持し、変換要求のスナップショットに含める。
- 入力中の予測は120msの待機後に非同期で要求。新しい入力で前の要求をキャンセルし、UI側でも入力識別子が一致する結果だけを受け取る。通常変換・確定・入力モード切り替え・フォーカス移動・IME終了後に古い予測が再表示されないようにした。候補が空の場合はウィンドウを隠す。
- ライブ変換オフ時の予測ではZenzaiを使わず、辞書から予測候補だけを返す。Tabは予測だけ、Spaceは通常候補だけを選択する。通常変換では不要な予測生成を無効化。ライブ変換オン時は差分入力を再利用し、Zenzai予測も設定に従う。
- Shift＋英字で始めた入力を確定まで英字で保持。小文字の英単語は通常変換候補に元の綴りを含める。入力・削除後の元データをSwiftから受け取り、F10で表示した文字列を確定処理にも反映する。

実設定を複製した隔離プロファイルで、CUDA・Zenzai有効・推論回数1の実サーバーに1文字ずつRPCを送信した。修正版では並行して12文字ごとに予測を要求した。最終的に配布用EXE・Swift DLLでも再確認した。以下はキー1個あたりのエンジン通信時間であり、TSFのアプリ上での描画時間は含まない。

| 版 | 入力したローマ字数 | 中央値 | 95パーセンタイル | 最大 |
| --- | ---: | ---: | ---: | ---: |
| alpha.4 | 60 | 67.43ms | 191.78ms | 393.40ms |
| alpha.5 | 60 | 0.53ms | 0.59ms | 0.67ms |
| alpha.5 | 240 | 0.59ms | 0.82ms | 1.27ms |

- 実IPCで、入力応答に候補が含まれないこと、並行予測の結果が予測だけであること、Windowsの各キーで英字を維持すること、小文字windowsが通常候補に含まれること、Spaceで漢字を取得すること、Tabで登録語の予測を取得することを確認。
- Rustの実Swiftエンジン回帰テスト、Swiftの入力・英字保持テスト、クライアントのテスト、古い予測を拒否する実UIサービスのテストを実施。全体の既存テスト・clippy・x64/x86チェック・設定画面テストとビルドも実施。
- 証跡: `input-latency-legacy.json`、`input-latency-new.json`、`async-server-new.log`、`test-async-final.log`、`swift-test-async-final.log`、`safe-async-input.log`（`target/verification`内）。検証用のサーバーと設定は通常入力から隔離し、終了後に検証プロセスを停止した。
- 更新後の実アプリでの長文入力、予測だけの表示、Space／Tab／F10とEnterの確定結果、Windowsの英字保持はユーザー確認待ち。旧DLLを切り替えるため、更新後もサインアウト・再サインインが必要。
- alpha.5の実更新は終了コード0で完了。全3,471ファイルのハッシュ、x64/x86のCOM登録、日本語IMEプロファイル、設定ショートカットを確認。既存設定は保持され、ライブ変換オフ・予測オン。インストール済みのエンジンPID 39064から読み取り専用の状態照会に応答を得た。
- 更新の証跡: `upgrade-alpha5-result.json`、`upgrade-alpha5.log`、`installer-alpha5-artifact.json`。

## alpha.6のGPU推論配置

- CUDAを選んでも、Windows版llama.cppの初期値によりモデル配置が`offloaded 0/13 layers to GPU`だった。GPUは検出され、演算用バッファも作られていたが、モデルとKVキャッシュはCPU側に置かれていた。
- 固定リビジョン`bbef9d2d`の`ZenzContext.createContext`にWindows用パッチを追加。GPUオフロード対応時は全層を要求し、非対応時とCPU専用ビルドは0層のまま。CUDA/Vulkanの実行ログで`offloaded 13/13 layers to GPU`とGPU側KVキャッシュを確認した。モデル、推論回数、候補数、候補評価方式は変更していない。
- GPU対応時は、変換終了ごとの推論コンテキスト破棄・再生成をやめ、`llama_kv_cache_clear`で前の文脈と系列データを消去する。Swift側の前回入力・プロンプト情報も空にする。GPUの演算用バッファを再利用する一方、CPUとCPU専用ビルドは従来の再生成処理を維持する。
- `cargo make`のSwiftビルドで固定リビジョンとパッチを検証・適用する。適用済みの場合は再適用せず、競合や異なるリビジョンは明示的に停止する。現在の依存ソース編集には編集ツールを使った。

RTX 4070 Ti環境で、インストール済みalpha.5サーバーと同じサーバーに修正版Swift DLLを組み合わせ、独立したプロファイル・パイプで計測した。Zenzai有効、同じQ5_K_Mモデル、推論回数1、ライブ変換オフ、学習オフ、同じ文脈。4文章を順番に8周変換し、初周を除く7回の中央値を比較した。各文章で入力が変わるため、同じ変換のキャッシュを繰り返し返す計測ではない。時間はSpace相当のConvertText RPC往復で、辞書処理・モデル評価・文脈再初期化を含む。純粋なモデル演算時間やアプリ描画時間ではない。

| 読みの文字数 | CUDA修正前 | CUDA修正後 | Vulkan修正前 | Vulkan修正後 |
| --- | ---: | ---: | ---: | ---: |
| 3 | 122.60ms | 23.30ms | 108.36ms | 25.44ms |
| 12 | 91.00ms | 27.79ms | 110.03ms | 29.79ms |
| 35 | 94.02ms | 28.22ms | 120.82ms | 31.41ms |
| 44 | 104.68ms | 33.28ms | 131.95ms | 36.90ms |

- CUDA/Vulkan/CPUすべてで4文章の全候補と並び順が修正前後で一致した。これは限定した回帰確認であり、すべての文章での変換品質を保証するものではない。
- CPUはGPU配置を変更しないため高速化の対象外。修正前の中央値408〜1,323msに対し、修正後432〜1,367msで、同程度の範囲だった。
- GPU配置だけの修正ではCUDA 35〜45ms、Vulkan 40〜52msだった。表はGPUバッファ再利用も含む結果。最初の変換要求はCUDA 53→43ms、Vulkan 44→30msだった。初回は変動があり、配置だけの修正時はVulkanで153msも観測した。エンジン起動からの総待ち時間は未計測。
- 実Swiftエンジンの辞書・学習・設定・FFI回帰テスト、計測コードのclippy、既存全体テスト・clippy・x64/x86チェック・設定画面テストとビルドを実施。
- 証跡: `target/verification/inference-{baseline,offload,reuse}-{cuda,vulkan,cpu}/results.json`と`server-error.log`、`inference-comparison.json`、`inference-reuse-comparison.json`、`test-gpu-engine.log`、`test-date-gpu-final.log`、`safe-gpu-offload.log`。再計測用は`crates/server/examples/inference_latency.rs`と`scripts/measure-inference.ps1`。
- 実アプリでの入力・Space変換の体感確認は残る。調査時点でメモ帳とExplorerがalpha.4 DLLを読み込んでいたため、alpha.5以降の非同期入力修正を反映するにはサインアウト・再サインインが必要。
- 最終配布用EXEとSwift DLLを使い、「きょう」を加えた5文章でも再確認した。CUDAは中央値23.21〜34.72ms、Vulkanは25.11〜37.22ms。「きょう」と「きょうはいいてんきですね」は通常の語句が先頭になり、日付は後ろに残った。実行ログで両GPUバックエンドの13/13層配置を検証した（`inference-packaged-{cuda,vulkan}/results.json`）。
- 最終配布版で並行予測を含む240キーの入力RPCを再計測し、中央値0.55ms、95パーセンタイル0.78ms、最大1.68msだった。英字保持、SpaceとTabの候補分離、「今日」が日付より前になることも実IPCで確認した。CPU/CUDAそれぞれで実モデルを有効にしたエンジン回帰テストが通った（`test-date-{cpu,gpu}-final.log`）。

## alpha.6の日付候補順

- 「きょう」の変換で「今日」より日付が先頭に出る問題を報告された。動的な日付・時刻候補を、通常変換の先頭へ挿入していたためだった。
- 通常の語句と登録語の優先を維持し、その後ろへ日付・時刻候補を挿入する。候補数に上限がある場合も通常候補を最低1件確保し、日付候補の枠を残す。外部変換プロバイダーの優先順は変更しない。
- 実Swiftエンジンを使うテストで修正前の先頭が日付になることを再現。修正後はSpace相当の変換と従来の入力経路で「今日」が先頭になり、日付も候補に残ること、候補数2でも「今日」と日付の両方を取得できることを確認した。
- 上の推論高速化の候補一致比較は同じRustサーバーでSwift DLLだけを差し替えた結果。最終版では、この日付候補順の修正が別途反映される。
- 証跡: `target/verification/test-date-priority-before.log`、`test-date-priority-after.log`、`test-date-gpu-final.log`。
- 実設定のZenzaiでは文脈によって「京」など別の語句が最良候補になるため、特定の単語を固定して昇格させてはいない。配布版IPCの回帰確認では「今日」が日付より前にあることを検証する。「きょう」を追加したSwift DLL単独比較でも、修正前後の候補は一致した（`inference-today-{baseline,reuse}-cuda/results.json`）。

## 生成した配布物

- 版: `0.1.0-alpha.6`
- インストーラー: `build/azookey-setup.exe` (511032526 bytes)
- SHA256: `F435A54C23AA8BFDB6E7EE1A206CCAB84BA74254032DE1E20CF032F50D4A400B`
- ハッシュ一覧: `build/release/manifest.json`、照合用: `build/azookey-setup.exe.sha256`

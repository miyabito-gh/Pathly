# Pathly

Pathly は、登録したファイル、フォルダー、HTTP/HTTPS URLを名前とタグで探して開くデスクトップランチャーです。URLは既定のブラウザで開きます。詳しい製品要件は[製品仕様](PRODUCT_SPEC.md)を参照してください。

## 開発起動

```powershell
npm install
npm run dev
```

Tauri 実行時は `npm run tauri dev` を使用します。

## 検索文字列を指定して起動

ビルドしたPathlyでは、オプション名を付けずに検索文字列を指定できます。空白を含む文字列は引用符で囲みます。

```powershell
.\Pathly.exe "設計資料 #重要"
```

すでにPathlyが起動している場合は新しいプロセスを残さず、既存ウィンドウに検索結果を表示します。

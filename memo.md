# メモ

## 更新間隔

* 1番短い間隔
* 最短5分

### 初回
1. TTL
   1. ないときは60分扱い
2. 直近1日の最短更新間隔

### 2回目以降

1. TTL
2. 前回の更新から投稿があった場合、経過時間/2
4. 前回の更新から投稿がなかった場合、経過時間*1.5
   1. 増加分は最大1時間
5. 6時間


## 旧設定からの変換

```sh
cat sources.yml | yq '.sources[] | .id as $i |[{"id":.id, "url":.source.feed, "token":.dest.mastodon.token, "tag":{"always":[], "ignore":.source.remote_keyword.ignore, "replace":.source.remote_keyword.replace_rules, "xpath":.source.remote_xpath_tags}}]'
```

## postgresの復元

```sh
pg_restore --no-owner -h localhost -U postgres -c -d mastaker mastaker.dump
```

## Rust

* とりあえず`clone()`でコピーしておくと動くけど、最適かは不明
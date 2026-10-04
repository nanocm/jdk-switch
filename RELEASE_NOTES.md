# jdk-switch v0.5.0

Choose a JDK distribution with `--vendor` in `jsh search` and `jsh download`. Eclipse Temurin remains the default; Amazon Corretto, Azul Zulu, and official OpenJDK builds are now available.

```console
jsh search 21 --vendor zulu
jsh download 21 --vendor corretto
```

Downloads are checked against the source's SHA-256 and file size. Temurin and OpenJDK can retry a matching archive from the Tsinghua and Huawei Cloud mirrors when their primary package links fail. Official OpenJDK archive builds are marked as archived in search results and before installation.

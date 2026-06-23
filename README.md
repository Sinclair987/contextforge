# ContextForge

ContextForge 是一个本地 Rust 命令行工具，用于扫描目录、提取 PDF/DOCX/EPUB/代码和常见文本文件，建立可增量更新的本地知识索引，检索相关内容，并生成适合提交给大语言模型的精简上下文包。

所有扫描、提取、排序、隐私检查和打包操作均在本机完成。

## 安装

在源码目录执行一次：

```powershell
cargo install --path . --locked
```

也可以直接从 GitHub 安装：

```powershell
cargo install --git https://github.com/Sinclair987/contextforge.git --locked
```

安装完成后，`contextforge` 可以在任意目录直接运行：

```powershell
contextforge --version
```

更新本地安装版本时使用：

```powershell
cargo install --path . --locked --force
```

## 快速使用

在需要分析的目录中运行：

```powershell
contextforge scan
contextforge search "所有权 借用"
contextforge audit
contextforge pack "所有权与借用"
```

`pack` 默认使用当前目录作为源目录、6000 estimated tokens 作为预算，并将结果写入当前目录下的 `contextforge-output`。

在任意位置分析其他目录：

```powershell
contextforge pack "期末大作业要求" -s D:\Study\Rust
```

只分析其中的 `FinalProject`，同时排除已有输出：

```powershell
contextforge pack "期末大作业要求" -s D:\Study\Rust --include FinalProject --exclude FinalProject\rust-final-pack
```

不需要进入 ContextForge 项目目录，也不需要输入 `target\release\contextforge.exe` 的完整路径。

## 常用命令

```powershell
# 查看支持的命令和参数
contextforge --help
contextforge pack --help

# 默认显示前 10 个片段，每个文件最多 2 个；0 表示不限制
contextforge search "ranking budget" --limit 20
contextforge search "ranking budget" --limit 0
contextforge search "齐泽克" --exact
contextforge search "木心" --type epub --per-file 1
contextforge search "ranking budget" --format json
contextforge search "ranking budget" --explain

# 管理本地知识索引
contextforge index build
contextforge index status
contextforge index status --verbose
contextforge index build --force
contextforge index clear

# 文本审计默认显示前 50 条，JSON 始终保留全部结果
contextforge audit --limit 100
contextforge audit --format json

# 预览选择结果，不写文件
contextforge pack "目标" --dry-run
contextforge pack "目标" --dry-run --explain

# 自定义预算和输出目录
contextforge pack "目标" -b 8000 -o .\my-context

# 脱敏并在选中内容含高风险发现时终止
contextforge pack "目标" --redact --fail-on high
```

旧格式仍然可用：

```powershell
contextforge pack --source . --goal "目标" --budget 6000 --output-dir out
```

## 配置

在目标目录运行：

```powershell
contextforge init
```

这会创建 `contextforge.toml`。命令会优先读取被分析目录中的配置文件；也可以使用全局 `--config <path>` 指定其他配置。

```toml
[scanner]
max_file_bytes = 8388608
max_document_bytes = 67108864
pdf_timeout_seconds = 5
ignore_patterns = [".git", "target", "node_modules", "dist", "build", "out", "demo-output", "venv"]
include_paths = []
exclude_paths = []

[output]
bundle = "context-bundle.md"
manifest = "context-manifest.json"
report = "context-report.md"
```

`include_paths` 和 `exclude_paths` 均相对于源目录，并可与命令行中的多个 `--include/--exclude` 叠加。普通文本、代码和数据文件默认最大 8 MiB；PDF、DOCX、EPUB 默认最大 64 MiB。两个限制均可按目录配置。程序最多同时解析 3 个 PDF，单个 PDF 默认最多解析 5 秒；超时后会跳过该文件并继续处理其他语料，可通过 `pdf_timeout_seconds` 调整。

## 本地知识索引

首次运行 `search` 时，ContextForge 会在源目录的 `.contextforge/index-v1.json` 建立词法索引。后续搜索先执行快速文件检查，只重新提取新增或发生变化的文件；未变化的 PDF、DOCX、EPUB 和文本片段直接复用。索引版本、提取规则或扫描配置变化时会自动重建。

搜索结果默认按文件分组，同一路径只显示一次，并隐藏分数、内部片段类型和排序原因。`--explain` 用于主动查看这些诊断信息。`--exact` 要求正文、标题或相对路径包含完整短语；`--type` 可以重复使用并支持 `pdf`、`docx`、`epub`、`markdown`、`text`、`code`、`data`、`markup` 和 `config`。

`.contextforge` 只保存在源目录中，扫描器会自动忽略它；目录内的 `.gitignore` 会阻止索引进入包含它的 Git 仓库。索引含有从文档中提取的本地文本，不应单独公开或发送。运行 `contextforge index clear` 可以安全删除索引，不会修改源文件。

## 输出文件

- `context-bundle.md`：提交给模型的精简资料，只包含目标、相对文件路径、合并后的行范围和正文。
- `context-manifest.json`：机器可读诊断信息，包含分数、预算决策、隐私统计和提取警告，不重复保存 bundle 正文。
- `context-report.md`：面向用户的简短摘要。

打包时会自动设置相关度底线，避免仅有零散关键词的弱匹配占满预算。若没有有效匹配，命令会返回错误且不会生成空 bundle。

Token 预算是跨模型的保守估算值：中文及其他非 ASCII 字符按单字符计入，ASCII 文本按约四字符一个 token 估算，并包含 bundle 标题、路径和行号开销。

隐私门禁只检查最终选中的内容；全目录发现仍单独记录为 source diagnostics。这样不会因为未进入 bundle 的无关文件而阻止打包。

## 支持格式

文档与文本：

- Markdown、TXT、日志、INI、CFG、properties、`.env*`
- PDF、DOCX、EPUB
- HTML、XML、SVG

配置与数据：

- TOML、JSON、YAML、CSV、TSV

代码：

- Rust、Python、JavaScript/TypeScript、Java、C/C++、C#、Go、Ruby、PHP、Swift、Kotlin、Scala
- Shell、PowerShell、SQL、Lua、R、Dart、Elixir、Clojure、F#/VB、Gradle
- Dockerfile、Makefile、Justfile、Gemfile、Jenkinsfile

### 文档限制

- PDF 必须包含可提取的文字层。扫描图片型 PDF 需要先使用 OCR 工具生成文字层。
- 加密、损坏、使用不受支持字体编码或无法解析的 PDF/DOCX/EPUB 会被跳过，并在终端、manifest 和 report 中显示提取警告，不会终止其他文件的处理。
- PDF 在隔离的子进程中解析；单个文件超时或底层解析器异常不会挂起、崩溃整次操作。
- DOCX 提取正文文字；复杂排版、批注、文本框及嵌入对象不会完整保留。
- EPUB 按 OPF spine 阅读顺序提取 XHTML 正文；DRM 保护、非 UTF-8 正文和复杂媒体内容不受支持。
- DOCX/EPUB 展开后的正文设有 128 MiB 安全上限，防止异常压缩包耗尽内存。
- ContextForge 提取的是文本语义，不保证复原原始页面排版。

## 工作流程

1. Scanner 递归扫描文件，并在读取内容前应用忽略及范围过滤。
2. Knowledge index 按文件元数据复用未变化的提取结果，只更新新增、修改或删除的记录。
3. Corpus loader 为需要处理的文件生成文本块、隐私发现和提取警告。
4. Ranker 使用 BM25/IDF、中文 n-gram、词项覆盖率、路径、标题和文件类型信号评分。
5. Budget planner 先保证多文件覆盖，再用高相关内容回填剩余预算。
6. Packer 生成精简 bundle、紧凑 manifest 和用户报告。

## 开发与验证

```powershell
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

主要模块：

- `src/scanner/`：目录扫描和范围过滤
- `src/extract/`：文本、PDF、DOCX 和标记语言提取
- `src/index/`：本地知识索引、增量刷新和状态管理
- `src/corpus.rs`：一次性语料加载和可恢复错误收集
- `src/chunk/`：结构化切块和 token 估算
- `src/rank/`：相关度评分
- `src/search/`：索引检索、过滤和结果分组
- `src/budget/`：预算与多文件选择
- `src/audit/`：隐私和指令覆盖检测
- `src/pack/`：bundle、manifest 和 report 生成

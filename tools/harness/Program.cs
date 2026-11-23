using System.Globalization;
using System.Text;
using System.Text.Json;
using Serilog;
using System.Text.Json.Serialization;
using Witcher3StringEditor.Contracts;
using Witcher3StringEditor.Miscellaneous;
using Witcher3StringEditor.Models;
using Witcher3StringEditor.Serializers;
using Witcher3StringEditor.Serializers.W3Strings;

// The recording harness: drives the original's serializer code with fixed inputs and writes
// deterministic outputs, so a port can be held to recorded bytes. Culture is pinned and the
// logger is silent: neither may leak into recorded output. This file also fixes the dump JSON
// escape rule the Rust side must mirror byte for byte:
//   - `"` -> \", `\` -> \\, \b \f \n \r \t named, other chars below 0x20 -> \u00xx lowercase hex
//   - a lone UTF-16 surrogate unit -> \uxxxx lowercase hex; a valid pair passes through as UTF-8
//   - everything else passes through as UTF-8; no BOM; compact, no indentation; LF newlines
CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
CultureInfo.DefaultThreadCurrentUICulture = CultureInfo.InvariantCulture;
Log.Logger = new LoggerConfiguration().MinimumLevel.Fatal().CreateLogger();

if (args.Length == 0)
{
    Console.Error.WriteLine(
        "usage: harness hash <key>" +
        " | decode <in.w3strings> <out.json>" +
        " | encode <in.json> <outdir> <langCode> <utf8|utf16>" +
        " | csv-decode <in> <out.json>" +
        " | csv-encode <in.json> <outdir> <langCode>" +
        " | gen-fixtures <dir>" +
        " | settings-default <out.json>" +
        " | settings-roundtrip <in.json> <out.json>");
    return 2;
}

try
{
    return args[0] switch
    {
        "hash" => CmdHash(args),
        "decode" => CmdDecode(args),
        "encode" => await CmdEncode(args),
        "csv-decode" => await CmdCsvDecode(args),
        "csv-encode" => await CmdCsvEncode(args),
        "gen-fixtures" => CmdGenFixtures(args),
        "settings-default" => CmdSettingsDefault(args),
        "settings-roundtrip" => CmdSettingsRoundtrip(args),
        _ => CmdUnknown(args[0])
    };
}
catch (Exception ex)
{
    Console.Out.WriteLine($"error: {ex.Message}");
    return 3;
}

static JsonSerializerOptions SettingsOptions() => new()
{
    WriteIndented = true,
    Converters =
    {
        new JsonStringEnumConverter(),
        new EncodingJsonConverter(),
        new LanguageJsonConverter(),
        new InterfaceJsonConverter<IRecentFileEntry, RecentItem>(),
        new InterfaceJsonConverter<IBackupItem, BackupItem>(),
        new ObservableCollectionJsonConverter<IBackupItem>(),
        new ObservableCollectionJsonConverter<IRecentFileEntry>()
    }
};

static int CmdSettingsDefault(string[] args)
{
    var settings = new AppSettings();
    File.WriteAllText(args[1], JsonSerializer.Serialize(settings, settings.GetType(), SettingsOptions()),
        new UTF8Encoding(false));
    return 0;
}

static int CmdSettingsRoundtrip(string[] args)
{
    var options = SettingsOptions();
    var settings = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(args[1]), options)
                   ?? new AppSettings();
    File.WriteAllText(args[2], JsonSerializer.Serialize(settings, settings.GetType(), options),
        new UTF8Encoding(false));
    return 0;
}

static int CmdUnknown(string name)
{
    Console.Error.WriteLine($"unknown command {name}");
    return 2;
}

static int CmdHash(string[] args)
{
    var hash = LocalizationKeyHash.Of(args.Length > 1 ? args[1] : string.Empty).Hash;
    Console.Out.WriteLine($"hash {hash} 0x{hash:X8}");
    return 0;
}

static int CmdDecode(string[] args) // decode <in.w3strings> <out.json>
{
    using var stream = File.OpenRead(args[1]);
    var storage = W3StringsStorage.Read(stream);
    var container = storage.ToContainer(stream);
    var items = W3StringsItemReader.Read(container);

    var writer = new StringWriter();
    writer.Write($"{{\"format\":\"w3strings\",\"version\":{storage.Version},\"magic\":\"0x{container.Magic:X8}\",\"languageKey\":\"0x{container.Key:X8}\"");
    writer.Write(",\"strings\":[");
    for (var i = 0; i < container.Strings.Count; i++)
    {
        var e = container.Strings[i];
        if (i > 0) writer.Write(',');
        writer.Write($"{{\"id\":{e.Id},\"offset\":{e.Offset},\"length\":{e.Length},\"text\":\"{Esc(e.Value)}\"}}");
    }
    writer.Write("],\"keys\":[");
    for (var i = 0; i < container.Keys.Count; i++)
    {
        var k = container.Keys[i];
        if (i > 0) writer.Write(',');
        writer.Write($"{{\"hash\":\"{k.KeyHash:X8}\",\"id\":{k.Id}}}");
    }
    writer.Write("],\"items\":[");
    for (var i = 0; i < items.Count; i++)
    {
        var it = items[i];
        if (i > 0) writer.Write(',');
        writer.Write($"{{\"strId\":\"{Esc(it.StrId)}\",\"keyHex\":\"{Esc(it.KeyHex)}\",\"keyName\":\"{Esc(it.KeyName)}\",\"oldText\":\"{Esc(it.OldText)}\",\"text\":\"{Esc(it.Text)}\"}}");
    }
    writer.Write("]}");
    File.WriteAllText(args[2], writer.ToString(), new UTF8Encoding(false));
    Console.Out.WriteLine($"ok {items.Count}");
    return 0;
}

static async Task<int> CmdEncode(string[] args) // encode <in.json> <outdir> <langCode> <utf8|utf16>
{
    var items = ParseItems(args[1]);
    var lang = RequireLanguage(args[3]);
    var encoding = args[4] switch
    {
        "utf8" => Encoding.UTF8,
        "utf16" => Encoding.Unicode,
        _ => throw new ArgumentException($"unknown encoding {args[4]}")
    };
    Directory.CreateDirectory(args[2]);
    var ok = await new W3StringsSerializer(new StubBackup()).Serialize(items,
        new W3StringSerializationOptions
        {
            OutputDirectory = args[2],
            TargetW3StringFileFormat = W3StringFileFormat.W3Strings,
            TargetW3Language = lang,
            Encoding = encoding
        });
    Console.Out.WriteLine(ok ? $"ok {items.Count}" : "fail");
    return ok ? 0 : 1;
}

static async Task<int> CmdCsvDecode(string[] args) // csv-decode <in> <out.json>
{
    var items = await new CsvSerializer(new StubBackup()).Deserialize(args[1]);
    var writer = new StringWriter();
    writer.Write("{\"format\":\"csv\",\"items\":[");
    for (var i = 0; i < items.Count; i++)
    {
        var it = items[i];
        if (i > 0) writer.Write(',');
        writer.Write($"{{\"strId\":\"{Esc(it.StrId)}\",\"keyHex\":\"{Esc(it.KeyHex)}\",\"keyName\":\"{Esc(it.KeyName)}\",\"oldText\":\"{Esc(it.OldText)}\",\"text\":\"{Esc(it.Text)}\"}}");
    }
    writer.Write("]}");
    File.WriteAllText(args[2], writer.ToString(), new UTF8Encoding(false));
    Console.Out.WriteLine($"ok {items.Count}");
    return 0;
}

static async Task<int> CmdCsvEncode(string[] args) // csv-encode <in.json> <outdir> <langCode>
{
    var items = ParseItems(args[1]);
    var lang = RequireLanguage(args[3]);
    Directory.CreateDirectory(args[2]);
    var ok = await new CsvSerializer(new StubBackup()).Serialize(items,
        new W3StringSerializationOptions
        {
            OutputDirectory = args[2],
            TargetW3StringFileFormat = W3StringFileFormat.Csv,
            TargetW3Language = lang,
            Encoding = Encoding.UTF8
        });
    Console.Out.WriteLine(ok ? $"ok {items.Count}" : "fail");
    return ok ? 0 : 1;
}

static int CmdGenFixtures(string[] args) // gen-fixtures <dir>
{
    var dir = args[1];
    Directory.CreateDirectory(dir);

    // Containers of both languages and both generations. The builder mutates the items it is
    // given, so every write builds from a fresh item list.
    foreach (var lang in new[] { "en", "cn" })
    foreach (var (version, name) in new[] { (162u, "162"), (164u, "164") })
    {
        var file = W3StringsBuilder.Build(TrapItems(), W3Language.FromCode(lang)!.Key)
                   ?? throw new InvalidOperationException($"the trap item set was refused for {lang}");
        using var stream = File.Create(Path.Combine(dir, $"{lang}-{name}.w3strings"));
        W3StringsStorage.Of(file, version).WriteAsync(stream).Wait();
    }

    // CSVs: the cn one carries the "cleartext" meta identifier of a magic-zero language.
    foreach (var lang in new[] { "en", "cn" })
        new CsvSerializer(new StubBackup()).Serialize(TrapItems(),
            new W3StringSerializationOptions
            {
                OutputDirectory = dir,
                TargetW3StringFileFormat = W3StringFileFormat.Csv,
                TargetW3Language = W3Language.FromCode(lang)!,
                Encoding = Encoding.UTF8
            }).Wait();

    // Corrupt containers cut from the en v164 one, per the behaviors the README documents.
    var good = File.ReadAllBytes(Path.Combine(dir, "en-164.w3strings"));
    var badMagic = (byte[])good.Clone();
    "XXXX"u8.CopyTo(badMagic);
    File.WriteAllBytes(Path.Combine(dir, "bad-magic.w3strings"), badMagic);
    File.WriteAllBytes(Path.Combine(dir, "truncated-head.w3strings"), good[..7]);
    File.WriteAllBytes(Path.Combine(dir, "truncated-buffer.w3strings"), good[..^12]);
    var community = (byte[])good.Clone();
    community[4] = 162; // declares the UTF-16 generation while holding UTF-8 texts
    File.WriteAllBytes(Path.Combine(dir, "community-162-utf8.w3strings"), community);

    // Encode inputs that exercise the builder's paths: named keys, hash-only, keyless,
    // inconsistent name/hash, invalid hex, a zero hash, duplicates, a bad id, empty.
    WriteItemsJson(Path.Combine(dir, "in-keynames.json"), TrapItems());
    WriteItemsJson(Path.Combine(dir, "in-hash-only.json"),
    [
        new HarnessItem { StrId = "0", KeyHex = "0B5A1E23", Text = "one" },
        new HarnessItem { StrId = "1", KeyHex = "FFFFFFFF", Text = "two" }
    ]);
    WriteItemsJson(Path.Combine(dir, "in-keyless.json"),
    [
        new HarnessItem { StrId = "7", Text = "no key" },
        new HarnessItem { StrId = "8", Text = "still no key" }
    ]);
    WriteItemsJson(Path.Combine(dir, "in-inconsistent.json"),
    [
        new HarnessItem { StrId = "0", KeyName = "label_menu", KeyHex = "DEADBEEF", Text = "name wins" }
    ]);
    WriteItemsJson(Path.Combine(dir, "in-invalid-hex.json"),
    [
        new HarnessItem { StrId = "0", KeyHex = "ZZZZ", Text = "invalid hex" },
        new HarnessItem { StrId = "1", KeyHex = "123", Text = "short hex" }
    ]);
    WriteItemsJson(Path.Combine(dir, "in-zero-key.json"),
    [
        new HarnessItem { StrId = "0", KeyHex = "00000000", Text = "zero hash" }
    ]);
    WriteItemsJson(Path.Combine(dir, "in-duplicates.json"),
    [
        new HarnessItem { StrId = "0", Text = "first" },
        new HarnessItem { StrId = "0", Text = "second" }
    ]);
    WriteItemsJson(Path.Combine(dir, "in-bad-strid.json"),
    [
        new HarnessItem { StrId = "not-a-number", Text = "bad id" }
    ]);
    WriteItemsJson(Path.Combine(dir, "in-empty.json"), []);

    foreach (var f in Directory.GetFiles(dir).OrderBy(f => f, StringComparer.Ordinal))
        Console.Out.WriteLine(Path.GetFileName(f));
    return 0;
}

static W3Language RequireLanguage(string code)
{
    return W3Language.FromCode(code)
           ?? throw new ArgumentException($"unknown language code {code}");
}

static List<IW3StringItem> ParseItems(string path)
{
    var options = new JsonSerializerOptions
    {
        PropertyNameCaseInsensitive = true,
        AllowTrailingCommas = true,
        ReadCommentHandling = JsonCommentHandling.Skip
    };
    var items = JsonSerializer.Deserialize<List<HarnessItem>>(File.ReadAllText(path), options)
                ?? throw new InvalidOperationException("no items in input");
    var result = new List<IW3StringItem>(items.Count);
    foreach (var item in items) result.Add(item);
    return result;
}

static List<IW3StringItem> TrapItems()
{
    return
    [
        new HarnessItem { StrId = "0", KeyName = "label_main_menu_new_game", Text = "New Game" },
        new HarnessItem { StrId = "1", KeyName = "LABEL_MAIN_MENU_OPTIONS", Text = "Options & <settings> + 'quoted'" },
        new HarnessItem { StrId = "2", KeyHex = "0B5A1E23", Text = "Hash-only key entry" },
        new HarnessItem { StrId = "3", Text = "Keyless entry, no key at all" },
        new HarnessItem { StrId = "1048576", KeyName = "geralt_quest", Text = "Wiedźmin — Żółw ĄŻŚĆĘŁŃÓŚŹŻ ą ż ś ć ę ł ń ó ś ź ż" },
        new HarnessItem { StrId = "4294967295", KeyName = "cn_text", Text = "杰洛特在陶森特公爵领地发现了新的兴趣点。" },
        new HarnessItem { StrId = "5", KeyName = "zh_hant_text", Text = "傑洛特在陶森特公爵領地發現了新的興趣點。" },
        new HarnessItem { StrId = "6", KeyName = "thai_text", Text = "เกมที่ดีที่สุดแห่งปี! รางวัลเกมยอดนิยม" },
        new HarnessItem { StrId = "7", KeyName = "emoji_text", Text = "Great game! 🎮⚔️🔥 family 👨‍👩‍👧‍👦 ZWJ" },
        new HarnessItem { StrId = "8", KeyName = "ffff_text", Text = "\uFFFF sentinel \uFFFD replacement \u00A0 nbsp" },
        new HarnessItem { StrId = "9", KeyName = "turkish_text", Text = "İstanbul ı şekli I İ i ı" },
        new HarnessItem { StrId = "10", KeyName = "tab_text", Text = "\tTabbed\ttext\twith\ttabs\nand\nnewlines\r\nand crlf" },
        new HarnessItem { StrId = "11", KeyName = "pipe_text", Text = "Text with | pipe ; semicolon = equals \" quotes \\" },
        new HarnessItem { StrId = "12", KeyName = "long_text", Text = new string('Ż', 1000) + new string('あ', 1000) + "end" },
        new HarnessItem { StrId = "13", KeyName = "empty_text", Text = "" },
        new HarnessItem { StrId = "14", KeyName = "greek_cyrillic", Text = "Ωμέγα Γκρέкκо Привет мир ЪъЫыЭэ" }
    ];
}

static void WriteItemsJson(string path, IReadOnlyList<IW3StringItem> items)
{
    var writer = new StringWriter();
    writer.Write('[');
    for (var i = 0; i < items.Count; i++)
    {
        var it = items[i];
        if (i > 0) writer.Write(',');
        writer.Write($"{{\"strId\":\"{Esc(it.StrId)}\",\"keyHex\":\"{Esc(it.KeyHex)}\",\"keyName\":\"{Esc(it.KeyName)}\",\"oldText\":\"{Esc(it.OldText)}\",\"text\":\"{Esc(it.Text)}\"}}");
    }
    writer.Write(']');
    File.WriteAllText(path, writer.ToString(), new UTF8Encoding(false));
}

static string Esc(string s)
{
    var sb = new StringBuilder(s.Length + 16);
    for (var i = 0; i < s.Length;)
    {
        var c = s[i];
        if (char.IsHighSurrogate(c) && i + 1 < s.Length && char.IsLowSurrogate(s[i + 1]))
        {
            sb.Append(c).Append(s[i + 1]); // a valid pair passes through as UTF-8
            i += 2;
            continue;
        }
        if (char.IsSurrogate(c))
        {
            sb.Append(CultureInfo.InvariantCulture, $"\\u{((int)c):x4}");
            i++;
            continue;
        }
        switch (c)
        {
            case '"': sb.Append("\\\""); break;
            case '\\': sb.Append("\\\\"); break;
            case '\b': sb.Append("\\b"); break;
            case '\f': sb.Append("\\f"); break;
            case '\n': sb.Append("\\n"); break;
            case '\r': sb.Append("\\r"); break;
            case '\t': sb.Append("\\t"); break;
            default:
                if (c < 0x20) sb.Append(CultureInfo.InvariantCulture, $"\\u{((int)c):x4}");
                else sb.Append(c);
                break;
        }
        i++;
    }
    return sb.ToString();
}

internal sealed class HarnessItem : IW3StringItem
{
    public string StrId { get; set; } = string.Empty;
    public string KeyHex { get; set; } = string.Empty;
    public string KeyName { get; set; } = string.Empty;
    public string OldText { get; set; } = string.Empty;
    public string Text { get; set; } = string.Empty;
}

internal sealed class StubBackup : IBackupService
{
    public Task<bool> Backup(string filePath) => Task.FromResult(true);
    public bool Restore(IBackupItem backupItem) => true;
    public bool Delete(IBackupItem backupItem) => true;
}

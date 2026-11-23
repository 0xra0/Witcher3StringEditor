# Port map: Witcher3StringEditor

0 of 59 units have a Rust port.

## Ports

None yet: `rust-anything bridge` starts one.

## Whole app

| Glue | Runs |
| --- | --- |
| `rust/app/src/units/settings.rs` | Rust, handing some cases to the original |
| `rust/app/src/units/w3codec.rs` | Rust, handing some cases to the original |

## Not ported yet

In the order `analyze` suggests.

| Unit | Lines | Exports | Readiness |
| --- | ---: | ---: | ---: |
| `Witcher3StringEditor.Dialogs/Views/` | 158 | 0 | 95 |
| `Witcher3StringEditor.Locales/CultureResolver.cs` | 99 | 0 | 90 |
| `Witcher3StringEditor.Contracts/IAppSettings.cs` | 50 | 0 | 99 |
| `Witcher3StringEditor.Dialogs/TemplateSelectors/` | 70 | 0 | 92 |
| `Witcher3StringEditor.Serializers/SavedTextFormatter.cs` | 42 | 0 | 99 |
| `Witcher3StringEditor.Contracts/ILogAccessService.cs` | 33 | 0 | 99 |
| `Witcher3StringEditor.Messaging/MessageTokens.cs` | 36 | 0 | 99 |
| `Witcher3StringEditor.Miscellaneous/AppPaths.cs` | 33 | 0 | 99 |
| `Witcher3StringEditor.Miscellaneous/EncodingJsonConverter.cs` | 43 | 0 | 97 |
| `Witcher3StringEditor.Miscellaneous/LanguageJsonConverter.cs` | 42 | 0 | 97 |
| `Witcher3StringEditor.Serializers/IW3StringFileSerializer.cs` | 36 | 0 | 99 |
| `Witcher3StringEditor.Serializers/W3StringItem.cs` | 42 | 0 | 97 |
| `Witcher3StringEditor.Serializers/W3StringSerializationOptions.cs` | 34 | 0 | 99 |
| `Witcher3StringEditor.Contracts/IBackupService.cs` | 26 | 0 | 99 |
| `Witcher3StringEditor.Contracts/W3Language.cs` | 118 | 3 | 84 |
| `Witcher3StringEditor.Contracts/W3StringFileFormat.cs` | 24 | 0 | 99 |
| `Witcher3StringEditor.Miscellaneous/ObservableCollectionJsonConverter.cs` | 33 | 0 | 97 |
| `Witcher3StringEditor.Contracts/IBackupItem.cs` | 32 | 0 | 95 |
| `Witcher3StringEditor.Contracts/IFileAssociationService.cs` | 21 | 0 | 99 |
| `Witcher3StringEditor.Contracts/IRecentFilesService.cs` | 22 | 0 | 99 |
| `Witcher3StringEditor.Contracts/ISettingsManagerService.cs` | 21 | 0 | 99 |
| `Witcher3StringEditor.Contracts/IStartupFileService.cs` | 20 | 0 | 100 |
| `Witcher3StringEditor.Contracts/IW3StringItem.cs` | 35 | 0 | 95 |
| `Witcher3StringEditor.Dialogs/Behaviors/` | 49 | 0 | 91 |
| `Witcher3StringEditor.Dictionary/DictionaryInfo.cs` | 17 | 0 | 100 |
| `Witcher3StringEditor.Miscellaneous/DebugHelper.cs` | 15 | 0 | 100 |
| `Witcher3StringEditor.Miscellaneous/InterfaceJsonConverter.cs` | 39 | 0 | 95 |
| `Witcher3StringEditor.Serializers/W3StringSerializerCoordinator.cs` | 111 | 0 | 81 |
| `Witcher3StringEditor/Providers/` | 66 | 0 | 81 |
| `Witcher3StringEditor.Contracts/IShellOpenService.cs` | 13 | 0 | 100 |
| `Witcher3StringEditor.Dictionary/AcDynamicDictionaryReplacer.cs` | 157 | 0 | 76 |
| `Witcher3StringEditor.Locales/CultureMatcher.cs` | 42 | 0 | 91 |
| `Witcher3StringEditor.Shared/Extensions/` | 105 | 0 | 83 |
| `Witcher3StringEditor/Properties/` | 9 | 0 | 100 |
| `tools/harness/Program.cs` | 360 | 0 | 69 |
| `Witcher3StringEditor.Contracts/IRecentFileEntry.cs` | 22 | 0 | 95 |
| `Witcher3StringEditor.Contracts/ITrackableW3StringItem.cs` | 13 | 0 | 98 |
| `Witcher3StringEditor.Dialogs/Models/` | 78 | 0 | 81 |
| `Witcher3StringEditor.Dictionary/DictionaryProvider.cs` | 186 | 0 | 75 |
| `Witcher3StringEditor.Dictionary/IDictionaryService.cs` | 5 | 0 | 98 |
| `Witcher3StringEditor.Serializers/CsvSerializer.cs` | 111 | 0 | 74 |
| `Witcher3StringEditor.Dialogs/Converters/` | 295 | 0 | 77 |
| `Witcher3StringEditor.Dictionary/DictionaryManager.cs` | 156 | 0 | 70 |
| `Witcher3StringEditor.Messaging/AsyncRequestMessage.cs` | 16 | 0 | 94 |
| `Witcher3StringEditor.Serializers/W3StringsSerializer.cs` | 127 | 0 | 78 |
| `Witcher3StringEditor/Factories/` | 203 | 0 | 78 |
| `Witcher3StringEditor.Locales/LangKeys.cs` | 11 | 0 | 92 |
| `Witcher3StringEditor.Serializers/ExcelSerializer.cs` | 218 | 0 | 70 |
| `Witcher3StringEditor.Dialogs/Comparers/` | 49 | 0 | 81 |
| `Witcher3StringEditor/Models/` | 307 | 0 | 69 |
| `Witcher3StringEditor/Views/` | 170 | 0 | 60 |
| `Witcher3StringEditor.Shared/Behaviors/` | 400 | 0 | 58 |
| `Witcher3StringEditor.Locales/Strings.Designer.cs` | 1366 | 0 | 64 |
| `Witcher3StringEditor/App.xaml.cs` | 385 | 0 | 42 |
| `Witcher3StringEditor.Serializers/W3Strings/` | 1436 | 0 | 9 |
| `Witcher3StringEditor/ViewModels/` | 721 | 0 | 8 |
| `Witcher3StringEditor/Services/` | 1262 | 0 | 0 |
| `Witcher3StringEditor/Cluster/` | 1247 | 0 | 0 |
| `Witcher3StringEditor.Dialogs/ViewModels/` | 1908 | 0 | 0 |

Regenerate with `rust-anything map --write`. Commit this file: when the original changes upstream, the next map marks every port whose recording it outdated.

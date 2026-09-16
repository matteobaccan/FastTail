use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    En,
    It,
    Fr,
    Es,
    Zh,
}

impl Language {
    pub fn name(&self) -> &'static str {
        match self {
            Language::En => "English",
            Language::It => "Italiano",
            Language::Fr => "Français",
            Language::Es => "Español",
            Language::Zh => "中文 (简体)",
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Language::En => "en",
            Language::It => "it",
            Language::Fr => "fr",
            Language::Es => "es",
            Language::Zh => "zh",
        }
    }

    pub fn from_code(code: &str) -> Self {
        if code.starts_with("it") {
            Language::It
        } else if code.starts_with("fr") {
            Language::Fr
        } else if code.starts_with("es") {
            Language::Es
        } else if code.starts_with("zh") {
            Language::Zh
        } else {
            Language::En
        }
    }

    pub fn detect() -> Self {
        // Check environment variables first
        for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let lower = val.to_lowercase();
                if lower.starts_with("it") {
                    return Language::It;
                } else if lower.starts_with("fr") {
                    return Language::Fr;
                } else if lower.starts_with("es") {
                    return Language::Es;
                } else if lower.starts_with("zh") {
                    return Language::Zh;
                }
            }
        }

        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }
            let lang_id = unsafe { GetUserDefaultUILanguage() };
            let primary_lang = lang_id & 0x3ff;
            match primary_lang {
                0x10 => return Language::It,
                0x0c => return Language::Fr,
                0x0a => return Language::Es,
                0x04 => return Language::Zh,
                _ => {}
            }
        }

        Language::En
    }
}

pub fn t(lang: Language, key: &str) -> &'static str {
    match (lang, key) {
        // Italian
        (Language::It, "app_subtitle") => "MONITORING LOG AD ALTE PRESTAZIONI",
        (Language::It, "follow_tail") => "SEGUI CODA (TAIL -F)",
        (Language::It, "paused") => "IN PAUSA",
        (Language::It, "tailing") => "IN ASCOLTO",
        (Language::It, "lines") => "Righe",
        (Language::It, "file_size") => "Dimensione",
        (Language::It, "throughput") => "Flusso",
        (Language::It, "cpu") => "CPU",
        (Language::It, "ram") => "RAM",
        (Language::It, "open_file") => "Apri File",
        (Language::It, "close_tab") => "Chiudi Tab",
        (Language::It, "filter_include") => "Includi (Regex)",
        (Language::It, "filter_exclude") => "Escludi (Regex)",
        (Language::It, "highlight_rules") => "Filtri di Colore",
        (Language::It, "filters") => "Filtri",
        (Language::It, "telemetry") => "Telemetria Sistema",
        (Language::It, "settings") => "Impostazioni",
        (Language::It, "theme") => "Tema Visivo",
        (Language::It, "language") => "Lingua",
        (Language::It, "screensaver") => "Screensaver Matrix",
        (Language::It, "screensaver_timeout") => "Attesa Screensaver (min)",
        (Language::It, "sound_fx") => "Effetti Sonori Cyber",
        (Language::It, "baretail_title") => "CONFIGURAZIONE BARETAIL RILEVATA",
        (Language::It, "baretail_desc") => "Sono state trovate impostazioni di BareTail nel Registro di sistema. Vuoi importare i file recenti e i colori di evidenziazione?",
        (Language::It, "baretail_import") => "IMPORTA DA BARETAIL",
        (Language::It, "baretail_skip") => "IGNORA",
        (Language::It, "json_expand") => "Espandi JSON",
        (Language::It, "json_collapse") => "Comprimi JSON",
        (Language::It, "search_placeholder") => "Cerca nel buffer (F3 avanti, Shift+F3 indietro)...",
        (Language::It, "add_rule") => "+ Aggiungi Filtro",
        (Language::It, "no_file_open") => "Nessun file aperto. Trascina qui un file .log o usa 'Apri File'.",
        (Language::It, "clear") => "Pulisci",
        (Language::It, "borderless") => "Finestra Senza Bordi",
        (Language::It, "show_lines") => "Mostra Numeri di Riga",
        (Language::It, "monitor_on") => "MONITORAGGIO: ATTIVO",
        (Language::It, "monitor_off") => "MONITORAGGIO: SOSPESO",
        (Language::It, "tip_follow_tail") => "Segui coda (Tail -f): scorre automaticamente all'ultima riga quando arrivano nuovi log. Clicca per mettere in pausa lo scorrimento e ispezionare lo storico.",
        (Language::It, "tip_monitor") => "Monitoraggio disco: legge attivamente gli aggiornamenti dal file. Sospendi per congelare l'anteprima senza consumare I/O su disco.",
        (Language::It, "view_mode_text") => "Testo (TXT)",
        (Language::It, "view_mode_hex") => "Binario (HEX)",
        (Language::It, "tip_view_mode") => "Passa tra vista Testo normale e Streaming Binario Esadecimale (HEX dump)",
        (Language::It, "hex_offset") => "Offset",
        (Language::It, "hex_bytes") => "Byte",
        (Language::It, "active_count") => "attivi",
        (Language::It, "color_filters_desc") => "Colorano il testo e lo sfondo delle righe per farle risaltare, senza nascondere le altre.",
        (Language::It, "visibility_filters_desc") => "Mostrano solo le righe pertinenti (Includi) o eliminano il rumore superfluo (Escludi).",
        (Language::It, "help") => "Guida",
        (Language::It, "recent_files") => "File Recenti",
        (Language::It, "clear_recent") => "Cancella cronologia recenti",
        (Language::It, "no_recent_files") => "Nessun file recente",
        (Language::It, "shortcuts_title") => "Guida e Scorciatoie da Tastiera",
        (Language::It, "font_size") => "Dimensione Carattere",
        (Language::It, "rules_order_hint") => "Le regole vengono valutate dall'alto in basso (si applica la prima regola valida). Usa ⬆/⬇ per riordinare.",
        (Language::It, "bold") => "Grassetto",
        (Language::It, "italic") => "Corsivo",
        (Language::It, "move_up") => "Sposta su",
        (Language::It, "move_down") => "Sposta giù",

        // French
        (Language::Fr, "app_subtitle") => "SURVEILLANCE DES JOURNAUX HAUTE PERFORMANCE",
        (Language::Fr, "follow_tail") => "SUIVRE LE FLUX (TAIL -F)",
        (Language::Fr, "paused") => "EN PAUSE",
        (Language::Fr, "tailing") => "EN ÉCOUTE",
        (Language::Fr, "lines") => "Lignes",
        (Language::Fr, "file_size") => "Taille",
        (Language::Fr, "throughput") => "Débit",
        (Language::Fr, "cpu") => "CPU",
        (Language::Fr, "ram") => "RAM",
        (Language::Fr, "open_file") => "Ouvrir Fichier",
        (Language::Fr, "close_tab") => "Fermer Onglet",
        (Language::Fr, "filter_include") => "Inclure (Regex)",
        (Language::Fr, "filter_exclude") => "Exclure (Regex)",
        (Language::Fr, "highlight_rules") => "Filtres de Couleur",
        (Language::Fr, "filters") => "Filtres",
        (Language::Fr, "telemetry") => "Télémétrie Système",
        (Language::Fr, "settings") => "Paramètres",
        (Language::Fr, "theme") => "Thème Visuel",
        (Language::Fr, "language") => "Langue",
        (Language::Fr, "screensaver") => "Écran de Veille Matrix",
        (Language::Fr, "screensaver_timeout") => "Délai Veille (min)",
        (Language::Fr, "sound_fx") => "Effets Sonores Cyber",
        (Language::Fr, "baretail_title") => "CONFIGURATION BARETAIL DÉTECTÉE",
        (Language::Fr, "baretail_desc") => "Paramètres BareTail trouvés dans le registre Windows. Importer les fichiers et règles de filtres ?",
        (Language::Fr, "baretail_import") => "IMPORTER DE BARETAIL",
        (Language::Fr, "baretail_skip") => "IGNORER",
        (Language::Fr, "json_expand") => "Développer JSON",
        (Language::Fr, "json_collapse") => "Réduire JSON",
        (Language::Fr, "search_placeholder") => "Rechercher dans le tampon (F3 suivant, Shift+F3 précédent)...",
        (Language::Fr, "add_rule") => "+ Ajouter Filtre",
        (Language::Fr, "no_file_open") => "Aucun fichier ouvert. Glissez-déposez un journal ici ou utilisez 'Ouvrir Fichier'.",
        (Language::Fr, "clear") => "Effacer",
        (Language::Fr, "borderless") => "Fenêtre Sans Bordure",
        (Language::Fr, "show_lines") => "Afficher Numéros de Ligne",
        (Language::Fr, "monitor_on") => "SURVEILLANCE: ACTIVE",
        (Language::Fr, "monitor_off") => "SURVEILLANCE: ARRÊTÉE",
        (Language::Fr, "tip_follow_tail") => "Suivre le flux (Tail -f): défile automatiquement vers les nouvelles lignes. Cliquez pour mettre en pause le défilement.",
        (Language::Fr, "tip_monitor") => "Surveillance disque: lit activement les mises à jour depuis le disque. Cliquez pour suspendre la lecture (gel du tampon) sans fermer le fichier.",
        (Language::Fr, "view_mode_text") => "Texte (TXT)",
        (Language::Fr, "view_mode_hex") => "Binaire (HEX)",
        (Language::Fr, "tip_view_mode") => "Basculer entre vue Texte et Streaming Binaire Hexadécimal (HEX dump)",
        (Language::Fr, "hex_offset") => "Décalage",
        (Language::Fr, "hex_bytes") => "Octets",
        (Language::Fr, "active_count") => "actifs",
        (Language::Fr, "color_filters_desc") => "Colorent le texte et l'arrière-plan des lignes pour les mettre en valeur sans masquer le reste.",
        (Language::Fr, "visibility_filters_desc") => "Affichent uniquement les lignes pertinentes (Inclure) ou masquent le bruit superflu (Exclure).",
        (Language::Fr, "help") => "Aide",
        (Language::Fr, "recent_files") => "Fichiers Récents",
        (Language::Fr, "clear_recent") => "Effacer les fichiers récents",
        (Language::Fr, "no_recent_files") => "Aucun fichier récent",
        (Language::Fr, "shortcuts_title") => "Aide et Raccourcis Clavier",
        (Language::Fr, "font_size") => "Taille de Police",
        (Language::Fr, "rules_order_hint") => "Les règles sont évaluées de haut en bas (la première règle valide s'applique). Utilisez ⬆/⬇ pour réordonner.",
        (Language::Fr, "bold") => "Gras",
        (Language::Fr, "italic") => "Italique",
        (Language::Fr, "move_up") => "Monter",
        (Language::Fr, "move_down") => "Descendre",

        // Spanish
        (Language::Es, "app_subtitle") => "MONITORIZACIÓN DE REGISTROS DE ALTO RENDIMIENTO",
        (Language::Es, "follow_tail") => "SEGUI COLA (TAIL -F)",
        (Language::Es, "paused") => "EN PAUSA",
        (Language::Es, "tailing") => "ESCUCHANDO",
        (Language::Es, "lines") => "Líneas",
        (Language::Es, "file_size") => "Tamaño",
        (Language::Es, "throughput") => "Rendimiento",
        (Language::Es, "cpu") => "CPU",
        (Language::Es, "ram") => "RAM",
        (Language::Es, "open_file") => "Abrir Archivo",
        (Language::Es, "close_tab") => "Cerrar Pestaña",
        (Language::Es, "filter_include") => "Incluir (Regex)",
        (Language::Es, "filter_exclude") => "Excluir (Regex)",
        (Language::Es, "highlight_rules") => "Filtros de Color",
        (Language::Es, "filters") => "Filtros",
        (Language::Es, "telemetry") => "Telemetría del Sistema",
        (Language::Es, "settings") => "Configuración",
        (Language::Es, "theme") => "Tema Visual",
        (Language::Es, "language") => "Idioma",
        (Language::Es, "screensaver") => "Salvapantallas Matrix",
        (Language::Es, "screensaver_timeout") => "Espera Salvapantallas (min)",
        (Language::Es, "sound_fx") => "Efectos de Sonido Cyber",
        (Language::Es, "baretail_title") => "CONFIGURACIÓN DE BARETAIL DETECTADA",
        (Language::Es, "baretail_desc") => "Se encontraron ajustes de BareTail en el registro. ¿Desea importar los archivos recientes y reglas de filtros?",
        (Language::Es, "baretail_import") => "IMPORTAR DESDE BARETAIL",
        (Language::Es, "baretail_skip") => "IGNORAR",
        (Language::Es, "json_expand") => "Expandir JSON",
        (Language::Es, "json_collapse") => "Contraer JSON",
        (Language::Es, "search_placeholder") => "Buscar en búfer (F3 siguiente, Shift+F3 anterior)...",
        (Language::Es, "add_rule") => "+ Añadir Filtro",
        (Language::Es, "no_file_open") => "Ningún archivo abierto. Arrastre un log aquí o pulse 'Abrir Archivo'.",
        (Language::Es, "clear") => "Limpiar",
        (Language::Es, "borderless") => "Ventana Sin Bordes",
        (Language::Es, "show_lines") => "Mostrar Números de Línea",
        (Language::Es, "monitor_on") => "MONITORIZACIÓN: ACTIVA",
        (Language::Es, "monitor_off") => "MONITORIZACIÓN: DETENIDA",
        (Language::Es, "tip_follow_tail") => "Seguir cola (Tail -f): se desplaza automáticamente a las líneas más recientes. Haga clic para pausar el desplazamiento.",
        (Language::Es, "tip_monitor") => "Monitorización de disco: lee activamente actualizaciones desde el disco. Haga clic para suspender la lectura (congelar instantánea) sin cerrar el archivo.",
        (Language::Es, "view_mode_text") => "Texto (TXT)",
        (Language::Es, "view_mode_hex") => "Binario (HEX)",
        (Language::Es, "tip_view_mode") => "Alternar entre vista de Texto normal y Streaming Binario Hexadecimal (HEX dump)",
        (Language::Es, "hex_offset") => "Offset",
        (Language::Es, "hex_bytes") => "Bytes",
        (Language::Es, "active_count") => "activos",
        (Language::Es, "color_filters_desc") => "Colorean el texto y fondo de las líneas correspondientes para resaltarlas sin ocultar las demás.",
        (Language::Es, "visibility_filters_desc") => "Muestran solo las líneas pertinentes (Incluir) u ocultan el ruido superfluo (Excluir).",
        (Language::Es, "help") => "Ayuda",
        (Language::Es, "recent_files") => "Archivos Recientes",
        (Language::Es, "clear_recent") => "Borrar historial reciente",
        (Language::Es, "no_recent_files") => "Sin archivos recientes",
        (Language::Es, "shortcuts_title") => "Ayuda y Atajos de Teclado",
        (Language::Es, "font_size") => "Tamaño de Fuente",
        (Language::Es, "rules_order_hint") => "Las reglas se evalúan de arriba a abajo (se aplica la primera regla válida). Use ⬆/⬇ para reordenar.",
        (Language::Es, "bold") => "Negrita",
        (Language::Es, "italic") => "Cursiva",
        (Language::Es, "move_up") => "Mover arriba",
        (Language::Es, "move_down") => "Mover abajo",

        // Chinese (Simplified)
        (Language::Zh, "app_subtitle") => "高性能日志实时监控终端",
        (Language::Zh, "follow_tail") => "实时追踪 (TAIL -F)",
        (Language::Zh, "paused") => "已暂停",
        (Language::Zh, "tailing") => "监控中",
        (Language::Zh, "lines") => "总行数",
        (Language::Zh, "file_size") => "文件大小",
        (Language::Zh, "throughput") => "吞吐量",
        (Language::Zh, "cpu") => "处理器",
        (Language::Zh, "ram") => "内存占用",
        (Language::Zh, "open_file") => "打开文件",
        (Language::Zh, "close_tab") => "关闭标签",
        (Language::Zh, "filter_include") => "包含规则 (正则)",
        (Language::Zh, "filter_exclude") => "排除规则 (正则)",
        (Language::Zh, "highlight_rules") => "颜色过滤规则",
        (Language::Zh, "filters") => "过滤器",
        (Language::Zh, "telemetry") => "系统实时遥测",
        (Language::Zh, "settings") => "设置",
        (Language::Zh, "theme") => "视觉主题",
        (Language::Zh, "language") => "界面语言",
        (Language::Zh, "screensaver") => "黑客帝国屏保 (Matrix)",
        (Language::Zh, "screensaver_timeout") => "屏保空闲时间 (分钟)",
        (Language::Zh, "sound_fx") => "赛博音效反馈",
        (Language::Zh, "baretail_title") => "检测到 BARETAIL 配置",
        (Language::Zh, "baretail_desc") => "在注册表中发现了现有的 BareTail 配置。是否导入历史文件与颜色过滤规则？",
        (Language::Zh, "baretail_import") => "从 BARETAIL 导入",
        (Language::Zh, "baretail_skip") => "跳过",
        (Language::Zh, "json_expand") => "展开 JSON",
        (Language::Zh, "json_collapse") => "收起 JSON",
        (Language::Zh, "search_placeholder") => "在日志中搜索 (F3 下一个, Shift+F3 上一个)...",
        (Language::Zh, "add_rule") => "+ 添加过滤规则",
        (Language::Zh, "no_file_open") => "未打开任何文件。请将日志文件拖拽至此处或点击“打开文件”。",
        (Language::Zh, "clear") => "清除",
        (Language::Zh, "borderless") => "无边框窗口模式",
        (Language::Zh, "show_lines") => "显示行号",
        (Language::Zh, "monitor_on") => "文件监控: 运行中",
        (Language::Zh, "monitor_off") => "文件监控: 已暂停",
        (Language::Zh, "tip_follow_tail") => "实时追踪 (Tail -f): 产生新日志时自动滚屏至最新行。点击可暂停自动滚屏以便查阅历史。",
        (Language::Zh, "tip_monitor") => "磁盘监控: 实时读取磁盘文件更新。点击可暂停磁盘读取（冻结当前快照）而不关闭文件。",
        (Language::Zh, "view_mode_text") => "文本 (TXT)",
        (Language::Zh, "view_mode_hex") => "二进制 (HEX)",
        (Language::Zh, "tip_view_mode") => "在普通文本视图与十六进制二进制流视图 (HEX dump) 之间切换",
        (Language::Zh, "hex_offset") => "偏移量",
        (Language::Zh, "hex_bytes") => "字节",
        (Language::Zh, "active_count") => "生效",
        (Language::Zh, "color_filters_desc") => "高亮着色匹配的行（前景色/背景色），醒目直观且不会隐藏其他日志。",
        (Language::Zh, "visibility_filters_desc") => "行级可见性过滤：仅显示匹配行 (包含) 或剔除冗余干扰日志 (排除)。",
        (Language::Zh, "help") => "帮助",
        (Language::Zh, "recent_files") => "最近文件",
        (Language::Zh, "clear_recent") => "清空最近文件记录",
        (Language::Zh, "no_recent_files") => "无最近文件",
        (Language::Zh, "shortcuts_title") => "使用指南与快捷键",
        (Language::Zh, "font_size") => "字体字号",
        (Language::Zh, "rules_order_hint") => "规则自上而下依次匹配（匹配首条规则即停）。使用 ⬆/⬇ 调整优先级。",
        (Language::Zh, "bold") => "粗体",
        (Language::Zh, "italic") => "斜体",
        (Language::Zh, "move_up") => "上移",
        (Language::Zh, "move_down") => "下移",

        // Default: English fallback
        (_, "app_subtitle") => "HIGH-PERFORMANCE REAL-TIME LOG MONITOR",
        (_, "follow_tail") => "FOLLOW TAIL (TAIL -F)",
        (_, "paused") => "PAUSED",
        (_, "tailing") => "STREAMING",
        (_, "lines") => "Lines",
        (_, "file_size") => "Size",
        (_, "throughput") => "Throughput",
        (_, "cpu") => "CPU",
        (_, "ram") => "RAM",
        (_, "open_file") => "Open File",
        (_, "close_tab") => "Close Tab",
        (_, "filter_include") => "Include (Regex)",
        (_, "filter_exclude") => "Exclude (Regex)",
        (_, "highlight_rules") => "Color Filters",
        (_, "filters") => "Filters",
        (_, "telemetry") => "System Telemetry",
        (_, "settings") => "Settings",
        (_, "theme") => "Visual Theme",
        (_, "language") => "Language",
        (_, "screensaver") => "Matrix Screensaver",
        (_, "screensaver_timeout") => "Screensaver Timeout (min)",
        (_, "sound_fx") => "Cyber Audio SFX",
        (_, "baretail_title") => "BARETAIL CONFIGURATION DETECTED",
        (_, "baretail_desc") => "Existing BareTail settings found in Windows Registry. Do you want to import recent files and filter rules?",
        (_, "baretail_import") => "IMPORT FROM BARETAIL",
        (_, "baretail_skip") => "SKIP",
        (_, "json_expand") => "Expand JSON",
        (_, "json_collapse") => "Collapse JSON",
        (_, "search_placeholder") => "Search buffer (F3 next, Shift+F3 prev)...",
        (_, "add_rule") => "+ Add Filter",
        (_, "no_file_open") => "No file open. Drag & drop a log file here or click 'Open File'.",
        (_, "clear") => "Clear",
        (_, "borderless") => "Borderless Window",
        (_, "show_lines") => "Show Line Numbers",
        (_, "monitor_on") => "MONITOR: ACTIVE",
        (_, "monitor_off") => "MONITOR: STOPPED",
        (_, "tip_follow_tail") => "Follow Tail (Tail -f): auto-scrolls view to newest lines when data arrives. Pause to stop auto-scrolling and inspect earlier log lines.",
        (_, "tip_monitor") => "Disk monitoring: actively reads file updates from disk. Click to stop disk polling (freeze buffer snapshot) without consuming disk I/O.",
        (_, "view_mode_text") => "Text (TXT)",
        (_, "view_mode_hex") => "Binary (HEX)",
        (_, "tip_view_mode") => "Switch between normal Text view and Binary Hex streaming view (HEX dump)",
        (_, "hex_offset") => "Offset",
        (_, "hex_bytes") => "Bytes",
        (_, "active_count") => "active",
        (_, "color_filters_desc") => "Highlights matching lines with custom text/background colors to stand out, without hiding other lines.",
        (_, "visibility_filters_desc") => "Line visibility filters: display only relevant lines (Include) or discard unwanted noise (Exclude).",
        (_, "help") => "Help",
        (_, "recent_files") => "Recent Files",
        (_, "clear_recent") => "Clear recent files history",
        (_, "no_recent_files") => "No recent files",
        (_, "shortcuts_title") => "Guide & Keyboard Shortcuts",
        (_, "font_size") => "Font Size",
        (_, "rules_order_hint") => "Rules evaluate from top to bottom (first match wins). Use ⬆/⬇ to reorder priority.",
        (_, "bold") => "Bold",
        (_, "italic") => "Italic",
        (_, "move_up") => "Move Up",
        (_, "move_down") => "Move Down",

        // Unknown key fallback
        _ => "Unknown",
    }
}

use crate::models::LanguagePreference;

/// Locale resolved from the user's language preference. The frontend and
/// backend intentionally share the same locale identifiers and fallbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    ZhCn,
    ZhTw,
    Es,
    PtBr,
    Ja,
    Ko,
    De,
    Fr,
    Ru,
    Hi,
    Ar,
    It,
    Pl,
    Tr,
    Vi,
}

fn resolve_system_locale(language: &str) -> Locale {
    let language = language.to_ascii_lowercase();
    if language.starts_with("zh-tw") || language.starts_with("zh-hk") {
        Locale::ZhTw
    } else if language.starts_with("zh") {
        Locale::ZhCn
    } else if language.starts_with("pt") {
        Locale::PtBr
    } else if language.starts_with("es") {
        Locale::Es
    } else if language.starts_with("ja") {
        Locale::Ja
    } else if language.starts_with("ko") {
        Locale::Ko
    } else if language.starts_with("de") {
        Locale::De
    } else if language.starts_with("fr") {
        Locale::Fr
    } else if language.starts_with("ru") {
        Locale::Ru
    } else if language.starts_with("hi") {
        Locale::Hi
    } else if language.starts_with("ar") {
        Locale::Ar
    } else if language.starts_with("it") {
        Locale::It
    } else if language.starts_with("pl") {
        Locale::Pl
    } else if language.starts_with("tr") {
        Locale::Tr
    } else if language.starts_with("vi") {
        Locale::Vi
    } else {
        Locale::En
    }
}

/// Resolve the effective locale from a language preference.
pub fn resolve(language: LanguagePreference) -> Locale {
    match language {
        LanguagePreference::System => sys_locale::get_locale()
            .as_deref()
            .map(resolve_system_locale)
            .unwrap_or(Locale::En),
        LanguagePreference::En => Locale::En,
        LanguagePreference::ZhCn => Locale::ZhCn,
        LanguagePreference::ZhTw => Locale::ZhTw,
        LanguagePreference::Es => Locale::Es,
        LanguagePreference::PtBr => Locale::PtBr,
        LanguagePreference::Ja => Locale::Ja,
        LanguagePreference::Ko => Locale::Ko,
        LanguagePreference::De => Locale::De,
        LanguagePreference::Fr => Locale::Fr,
        LanguagePreference::Ru => Locale::Ru,
        LanguagePreference::Hi => Locale::Hi,
        LanguagePreference::Ar => Locale::Ar,
        LanguagePreference::It => Locale::It,
        LanguagePreference::Pl => Locale::Pl,
        LanguagePreference::Tr => Locale::Tr,
        LanguagePreference::Vi => Locale::Vi,
    }
}

/// Translate a dictionary key into the given locale. Unknown keys render as
/// themselves so a missing entry degrades gracefully instead of panicking.
pub fn tr(locale: Locale, key: &'static str) -> &'static str {
    match (locale, key) {
        (Locale::En, "menu.open") => "Open OpenQuota",
        (Locale::ZhCn, "menu.open") => "打开 OpenQuota",
        (Locale::ZhTw, "menu.open") => "打開 OpenQuota",
        (Locale::Es, "menu.open") => "Abrir OpenQuota",
        (Locale::PtBr, "menu.open") => "Abrir OpenQuota",
        (Locale::Ja, "menu.open") => "OpenQuota を開く",
        (Locale::Ko, "menu.open") => "OpenQuota 열기",
        (Locale::De, "menu.open") => "OpenQuota öffnen",
        (Locale::Fr, "menu.open") => "Ouvrir OpenQuota",
        (Locale::Ru, "menu.open") => "Открыть OpenQuota",
        (Locale::Hi, "menu.open") => "OpenQuota खोलें",
        (Locale::Ar, "menu.open") => "فتح OpenQuota",
        (Locale::It, "menu.open") => "Apri OpenQuota",
        (Locale::Pl, "menu.open") => "Otwórz OpenQuota",
        (Locale::Tr, "menu.open") => "OpenQuota'yu aç",
        (Locale::Vi, "menu.open") => "Mở OpenQuota",

        (Locale::En, "menu.customize") => "Customize…",
        (Locale::ZhCn, "menu.customize") => "自定义…",
        (Locale::ZhTw, "menu.customize") => "自訂…",
        (Locale::Es, "menu.customize") => "Personalizar…",
        (Locale::PtBr, "menu.customize") => "Personalizar…",
        (Locale::Ja, "menu.customize") => "カスタマイズ…",
        (Locale::Ko, "menu.customize") => "사용자 지정…",
        (Locale::De, "menu.customize") => "Anpassen…",
        (Locale::Fr, "menu.customize") => "Personnaliser…",
        (Locale::Ru, "menu.customize") => "Настроить…",
        (Locale::Hi, "menu.customize") => "कस्टमाइज़ करें…",
        (Locale::Ar, "menu.customize") => "تخصيص…",
        (Locale::It, "menu.customize") => "Personalizza…",
        (Locale::Pl, "menu.customize") => "Dostosuj…",
        (Locale::Tr, "menu.customize") => "Özelleştir…",
        (Locale::Vi, "menu.customize") => "Tùy chỉnh…",

        (Locale::En, "menu.settings") => "Settings…",
        (Locale::ZhCn, "menu.settings") => "设置…",
        (Locale::ZhTw, "menu.settings") => "設定…",
        (Locale::Es, "menu.settings") => "Configuración…",
        (Locale::PtBr, "menu.settings") => "Configurações…",
        (Locale::Ja, "menu.settings") => "設定…",
        (Locale::Ko, "menu.settings") => "설정…",
        (Locale::De, "menu.settings") => "Einstellungen…",
        (Locale::Fr, "menu.settings") => "Réglages…",
        (Locale::Ru, "menu.settings") => "Настройки…",
        (Locale::Hi, "menu.settings") => "सेटिंग्स…",
        (Locale::Ar, "menu.settings") => "الإعدادات…",
        (Locale::It, "menu.settings") => "Impostazioni…",
        (Locale::Pl, "menu.settings") => "Ustawienia…",
        (Locale::Tr, "menu.settings") => "Ayarlar…",
        (Locale::Vi, "menu.settings") => "Cài đặt…",

        (Locale::En, "menu.settings_short") => "Settings",
        (Locale::ZhCn, "menu.settings_short") => "设置",
        (Locale::ZhTw, "menu.settings_short") => "設定",
        (Locale::Es, "menu.settings_short") => "Configuración",
        (Locale::PtBr, "menu.settings_short") => "Configurações",
        (Locale::Ja, "menu.settings_short") => "設定",
        (Locale::Ko, "menu.settings_short") => "설정",
        (Locale::De, "menu.settings_short") => "Einstellungen",
        (Locale::Fr, "menu.settings_short") => "Réglages",
        (Locale::Ru, "menu.settings_short") => "Настройки",
        (Locale::Hi, "menu.settings_short") => "सेटिंग्स",
        (Locale::Ar, "menu.settings_short") => "الإعدادات",
        (Locale::It, "menu.settings_short") => "Impostazioni",
        (Locale::Pl, "menu.settings_short") => "Ustawienia",
        (Locale::Tr, "menu.settings_short") => "Ayarlar",
        (Locale::Vi, "menu.settings_short") => "Cài đặt",

        (Locale::En, "menu.quit") => "Quit OpenQuota",
        (Locale::ZhCn, "menu.quit") => "退出 OpenQuota",
        (Locale::ZhTw, "menu.quit") => "結束 OpenQuota",
        (Locale::Es, "menu.quit") => "Salir de OpenQuota",
        (Locale::PtBr, "menu.quit") => "Sair do OpenQuota",
        (Locale::Ja, "menu.quit") => "OpenQuota を終了",
        (Locale::Ko, "menu.quit") => "OpenQuota 종료",
        (Locale::De, "menu.quit") => "OpenQuota beenden",
        (Locale::Fr, "menu.quit") => "Quitter OpenQuota",
        (Locale::Ru, "menu.quit") => "Выйти из OpenQuota",
        (Locale::Hi, "menu.quit") => "OpenQuota छोड़ें",
        (Locale::Ar, "menu.quit") => "إنهاء OpenQuota",
        (Locale::It, "menu.quit") => "Esci da OpenQuota",
        (Locale::Pl, "menu.quit") => "Zamknij OpenQuota",
        (Locale::Tr, "menu.quit") => "OpenQuota'dan çık",
        (Locale::Vi, "menu.quit") => "Thoát OpenQuota",

        (Locale::En, "notif.open") => "Open OpenQuota",
        (Locale::ZhCn, "notif.open") => "打开 OpenQuota",
        (Locale::ZhTw, "notif.open") => "打開 OpenQuota",
        (Locale::Es, "notif.open") => "Abrir OpenQuota",
        (Locale::PtBr, "notif.open") => "Abrir OpenQuota",
        (Locale::Ja, "notif.open") => "OpenQuota を開く",
        (Locale::Ko, "notif.open") => "OpenQuota 열기",
        (Locale::De, "notif.open") => "OpenQuota öffnen",
        (Locale::Fr, "notif.open") => "Ouvrir OpenQuota",
        (Locale::Ru, "notif.open") => "Открыть OpenQuota",
        (Locale::Hi, "notif.open") => "OpenQuota खोलें",
        (Locale::Ar, "notif.open") => "فتح OpenQuota",
        (Locale::It, "notif.open") => "Apri OpenQuota",
        (Locale::Pl, "notif.open") => "Otwórz OpenQuota",
        (Locale::Tr, "notif.open") => "OpenQuota'yu aç",
        (Locale::Vi, "notif.open") => "Mở OpenQuota",

        (Locale::En, "notif.almost_out") => "Almost Out",
        (Locale::ZhCn, "notif.almost_out") => "即将用尽",
        (Locale::ZhTw, "notif.almost_out") => "即將用盡",
        (Locale::Es, "notif.almost_out") => "Casi agotado",
        (Locale::PtBr, "notif.almost_out") => "Quase esgotado",
        (Locale::Ja, "notif.almost_out") => "残りわずか",
        (Locale::Ko, "notif.almost_out") => "거의 소หมด",
        (Locale::De, "notif.almost_out") => "Fast aufgebraucht",
        (Locale::Fr, "notif.almost_out") => "Presque épuisé",
        (Locale::Ru, "notif.almost_out") => "Почти исчерпано",
        (Locale::Hi, "notif.almost_out") => "लगभग समाप्त",
        (Locale::Ar, "notif.almost_out") => "أوشك على النفاد",
        (Locale::It, "notif.almost_out") => "Quasi esaurito",
        (Locale::Pl, "notif.almost_out") => "Prawie wyczerpane",
        (Locale::Tr, "notif.almost_out") => "Neredeyse tükendi",
        (Locale::Vi, "notif.almost_out") => "Sắp hết",

        (Locale::En, "notif.almost_out_body") => "Under 10% usage remaining for this window.",
        (Locale::ZhCn, "notif.almost_out_body") => "此周期剩余用量不足 10%。",
        (Locale::ZhTw, "notif.almost_out_body") => "此週期剩餘用量不足 10%。",
        (Locale::Es, "notif.almost_out_body") => "Queda menos del 10 % de uso en esta ventana.",
        (Locale::PtBr, "notif.almost_out_body") => "Restam menos de 10% de uso nesta janela.",
        (Locale::Ja, "notif.almost_out_body") => "この期間の残り使用量は 10% 未満です。",
        (Locale::Ko, "notif.almost_out_body") => "이 기간의 사용량이 10% 미만 남았습니다.",
        (Locale::De, "notif.almost_out_body") => {
            "Für dieses Zeitfenster sind weniger als 10 % Nutzung übrig."
        }
        (Locale::Fr, "notif.almost_out_body") => {
            "Il reste moins de 10 % d'utilisation pour cette fenêtre."
        }
        (Locale::Ru, "notif.almost_out_body") => {
            "В этом окне осталось менее 10% доступного использования."
        }
        (Locale::Hi, "notif.almost_out_body") => "इस विंडो में 10% से कम उपयोग शेष है।",
        (Locale::Ar, "notif.almost_out_body") => "تبقى أقل من 10% من الاستخدام لهذه الفترة.",
        (Locale::It, "notif.almost_out_body") => {
            "Per questa finestra resta meno del 10% di utilizzo."
        }
        (Locale::Pl, "notif.almost_out_body") => {
            "W tym oknie pozostało mniej niż 10% wykorzystania."
        }
        (Locale::Tr, "notif.almost_out_body") => "Bu pencerede %10'dan az kullanım kaldı.",
        (Locale::Vi, "notif.almost_out_body") => {
            "Còn dưới 10% hạn mức sử dụng trong khoảng thời gian này."
        }

        (Locale::En, "notif.cutting_it_close") => "Cutting It Close",
        (Locale::ZhCn, "notif.cutting_it_close") => "接近上限",
        (Locale::ZhTw, "notif.cutting_it_close") => "接近上限",
        (Locale::Es, "notif.cutting_it_close") => "Al límite",
        (Locale::PtBr, "notif.cutting_it_close") => "No limite",
        (Locale::Ja, "notif.cutting_it_close") => "上限間近",
        (Locale::Ko, "notif.cutting_it_close") => "한도 임박",
        (Locale::De, "notif.cutting_it_close") => "Knapp am Limit",
        (Locale::Fr, "notif.cutting_it_close") => "Presque à la limite",
        (Locale::Ru, "notif.cutting_it_close") => "Почти достигнут лимит",
        (Locale::Hi, "notif.cutting_it_close") => "सीमा के करीब",
        (Locale::Ar, "notif.cutting_it_close") => "الاقتراب من الحد",
        (Locale::It, "notif.cutting_it_close") => "Quasi al limite",
        (Locale::Pl, "notif.cutting_it_close") => "Blisko limitu",
        (Locale::Tr, "notif.cutting_it_close") => "Limite çok yakın",
        (Locale::Vi, "notif.cutting_it_close") => "Sắp chạm giới hạn",

        (Locale::En, "notif.cutting_it_close_body") => "Projected to finish close to your limit.",
        (Locale::ZhCn, "notif.cutting_it_close_body") => "预计将接近你的用量上限。",
        (Locale::ZhTw, "notif.cutting_it_close_body") => "預計將接近你的用量上限。",
        (Locale::Es, "notif.cutting_it_close_body") => "Se prevé que termine cerca de tu límite.",
        (Locale::PtBr, "notif.cutting_it_close_body") => {
            "A previsão é que termine perto do seu limite."
        }
        (Locale::Ja, "notif.cutting_it_close_body") => {
            "上限に近い状態で終了すると予測されています。"
        }
        (Locale::Ko, "notif.cutting_it_close_body") => {
            "한도에 가까운 상태로 끝날 것으로 예상됩니다."
        }
        (Locale::De, "notif.cutting_it_close_body") => {
            "Es wird erwartet, dass das Limit fast erreicht wird."
        }
        (Locale::Fr, "notif.cutting_it_close_body") => "La limite devrait être presque atteinte.",
        (Locale::Ru, "notif.cutting_it_close_body") => {
            "Ожидается, что использование завершится почти у самого лимита."
        }
        (Locale::Hi, "notif.cutting_it_close_body") => {
            "अनुमान है कि उपयोग आपकी सीमा के करीब समाप्त होगा।"
        }
        (Locale::Ar, "notif.cutting_it_close_body") => {
            "من المتوقع أن ينتهي الاستخدام بالقرب من الحد المسموح."
        }
        (Locale::It, "notif.cutting_it_close_body") => "Si prevede che termini vicino al limite.",
        (Locale::Pl, "notif.cutting_it_close_body") => "Przewiduje się zakończenie blisko limitu.",
        (Locale::Tr, "notif.cutting_it_close_body") => {
            "Kullanımın limitinize yakın bir noktada bitmesi bekleniyor."
        }
        (Locale::Vi, "notif.cutting_it_close_body") => "Dự kiến sẽ chạm gần giới hạn của bạn.",

        (Locale::En, "notif.will_run_out") => "Will Run Out",
        (Locale::ZhCn, "notif.will_run_out") => "将要用尽",
        (Locale::ZhTw, "notif.will_run_out") => "即將用盡",
        (Locale::Es, "notif.will_run_out") => "Se agotará",
        (Locale::PtBr, "notif.will_run_out") => "Vai acabar",
        (Locale::Ja, "notif.will_run_out") => "使い切る見込み",
        (Locale::Ko, "notif.will_run_out") => "소진될 예정",
        (Locale::De, "notif.will_run_out") => "Wird aufgebraucht",
        (Locale::Fr, "notif.will_run_out") => "Épuisement prévu",
        (Locale::Ru, "notif.will_run_out") => "Скоро закончится",
        (Locale::Hi, "notif.will_run_out") => "समाप्त होने वाला है",
        (Locale::Ar, "notif.will_run_out") => "سينفد",
        (Locale::It, "notif.will_run_out") => "Si esaurirà",
        (Locale::Pl, "notif.will_run_out") => "Wkrótce się wyczerpie",
        (Locale::Tr, "notif.will_run_out") => "Tükenecek",
        (Locale::Vi, "notif.will_run_out") => "Sẽ hết",

        (Locale::En, "notif.will_run_out_body") => "Projected to run out before the limit resets.",
        (Locale::ZhCn, "notif.will_run_out_body") => "预计将在限额重置前用尽。",
        (Locale::ZhTw, "notif.will_run_out_body") => "預計將在限額重設前用盡。",
        (Locale::Es, "notif.will_run_out_body") => {
            "Se prevé que se agote antes de que se restablezca el límite."
        }
        (Locale::PtBr, "notif.will_run_out_body") => {
            "A previsão é que se esgote antes da redefinição do limite."
        }
        (Locale::Ja, "notif.will_run_out_body") => {
            "上限がリセットされる前に使い切ると予測されています。"
        }
        (Locale::Ko, "notif.will_run_out_body") => {
            "한도가 재설정되기 전에 소진될 것으로 예상됩니다."
        }
        (Locale::De, "notif.will_run_out_body") => {
            "Es wird erwartet, dass das Limit vor der Zurücksetzung aufgebraucht ist."
        }
        (Locale::Fr, "notif.will_run_out_body") => {
            "La limite devrait être épuisée avant sa réinitialisation."
        }
        (Locale::Ru, "notif.will_run_out_body") => "Ожидается, что лимит закончится до сброса.",
        (Locale::Hi, "notif.will_run_out_body") => {
            "अनुमान है कि सीमा रीसेट होने से पहले उपयोग समाप्त हो जाएगा।"
        }
        (Locale::Ar, "notif.will_run_out_body") => {
            "من المتوقع أن ينفد الاستخدام قبل إعادة تعيين الحد."
        }
        (Locale::It, "notif.will_run_out_body") => {
            "Si prevede che si esaurisca prima del ripristino del limite."
        }
        (Locale::Pl, "notif.will_run_out_body") => {
            "Przewiduje się wyczerpanie przed zresetowaniem limitu."
        }
        (Locale::Tr, "notif.will_run_out_body") => {
            "Limit sıfırlanmadan önce tükeneceği tahmin ediliyor."
        }
        (Locale::Vi, "notif.will_run_out_body") => {
            "Dự kiến sẽ hết trước khi giới hạn được đặt lại."
        }

        (_, _) => key,
    }
}

/// 菜单栏 / 任务栏实例右键菜单动作文案（Windows taskband 与 macOS
/// multiline-menubar 共用同一组 hide/refresh/settings/quit 动作）。
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn bar_action_label(locale: Locale, action: &str, agent_name: &str) -> String {
    match (locale, action) {
        (Locale::En, "hide") => format!("Hide {agent_name}"),
        (Locale::ZhCn, "hide") => format!("隐藏 {agent_name}"),
        (Locale::ZhTw, "hide") => format!("隱藏 {agent_name}"),
        (Locale::Es, "hide") => format!("Ocultar {agent_name}"),
        (Locale::PtBr, "hide") => format!("Ocultar {agent_name}"),
        (Locale::Ja, "hide") => format!("{agent_name} を非表示"),
        (Locale::Ko, "hide") => format!("{agent_name} 숨기기"),
        (Locale::De, "hide") => format!("{agent_name} ausblenden"),
        (Locale::Fr, "hide") => format!("Masquer {agent_name}"),
        (Locale::Ru, "hide") => format!("Скрыть {agent_name}"),
        (Locale::Hi, "hide") => format!("{agent_name} छिपाएँ"),
        (Locale::Ar, "hide") => format!("إخفاء {agent_name}"),
        (Locale::It, "hide") => format!("Nascondi {agent_name}"),
        (Locale::Pl, "hide") => format!("Ukryj {agent_name}"),
        (Locale::Tr, "hide") => format!("{agent_name} gizle"),
        (Locale::Vi, "hide") => format!("Ẩn {agent_name}"),

        (Locale::En, "refresh") => format!("Refresh {agent_name}"),
        (Locale::ZhCn, "refresh") => format!("刷新 {agent_name}"),
        (Locale::ZhTw, "refresh") => format!("重新整理 {agent_name}"),
        (Locale::Es, "refresh") => format!("Actualizar {agent_name}"),
        (Locale::PtBr, "refresh") => format!("Atualizar {agent_name}"),
        (Locale::Ja, "refresh") => format!("{agent_name} を更新"),
        (Locale::Ko, "refresh") => format!("{agent_name} 새로 고침"),
        (Locale::De, "refresh") => format!("{agent_name} aktualisieren"),
        (Locale::Fr, "refresh") => format!("Actualiser {agent_name}"),
        (Locale::Ru, "refresh") => format!("Обновить {agent_name}"),
        (Locale::Hi, "refresh") => format!("{agent_name} रीफ़्रेश करें"),
        (Locale::Ar, "refresh") => format!("تحديث {agent_name}"),
        (Locale::It, "refresh") => format!("Aggiorna {agent_name}"),
        (Locale::Pl, "refresh") => format!("Odśwież {agent_name}"),
        (Locale::Tr, "refresh") => format!("{agent_name} yenile"),
        (Locale::Vi, "refresh") => format!("Làm mới {agent_name}"),

        (Locale::En, "settings") => format!("Settings for {agent_name}…"),
        (Locale::ZhCn, "settings") => format!("设置 {agent_name}…"),
        (Locale::ZhTw, "settings") => format!("設定 {agent_name}…"),
        (Locale::Es, "settings") => format!("Configuración de {agent_name}…"),
        (Locale::PtBr, "settings") => format!("Configurações de {agent_name}…"),
        (Locale::Ja, "settings") => format!("{agent_name} の設定…"),
        (Locale::Ko, "settings") => format!("{agent_name} 설정…"),
        (Locale::De, "settings") => format!("Einstellungen für {agent_name}…"),
        (Locale::Fr, "settings") => format!("Réglages de {agent_name}…"),
        (Locale::Ru, "settings") => format!("Настройки {agent_name}…"),
        (Locale::Hi, "settings") => format!("{agent_name} की सेटिंग्स…"),
        (Locale::Ar, "settings") => format!("إعدادات {agent_name}…"),
        (Locale::It, "settings") => format!("Impostazioni per {agent_name}…"),
        (Locale::Pl, "settings") => format!("Ustawienia {agent_name}…"),
        (Locale::Tr, "settings") => format!("{agent_name} ayarları…"),
        (Locale::Vi, "settings") => format!("Cài đặt {agent_name}…"),

        (locale, "quit") => tr(locale, "menu.quit").to_owned(),
        (_, other) => other.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve, tr, Locale};
    use crate::models::LanguagePreference;

    #[test]
    fn explicit_and_system_locale_resolution_cover_target_languages() {
        assert_eq!(resolve(LanguagePreference::ZhCn), Locale::ZhCn);
        assert_eq!(resolve(LanguagePreference::ZhTw), Locale::ZhTw);
        assert_eq!(resolve(LanguagePreference::PtBr), Locale::PtBr);
        assert_eq!(resolve(LanguagePreference::Ar), Locale::Ar);
        assert_eq!(resolve(LanguagePreference::Vi), Locale::Vi);
    }

    #[test]
    fn translated_native_labels_are_available() {
        assert_eq!(tr(Locale::ZhCn, "menu.settings"), "设置…");
        assert_eq!(tr(Locale::Ja, "menu.settings"), "設定…");
        assert_eq!(tr(Locale::Ar, "notif.almost_out"), "أوشك على النفاد");
        assert_eq!(tr(Locale::En, "menu.settings"), "Settings…");
    }

    #[test]
    fn unknown_key_renders_itself() {
        assert_eq!(tr(Locale::ZhCn, "menu.unknown"), "menu.unknown");
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn bar_context_menu_labels_use_the_agent_name() {
        assert_eq!(
            super::bar_action_label(Locale::En, "hide", "Claude"),
            "Hide Claude"
        );
        assert_eq!(
            super::bar_action_label(Locale::ZhCn, "hide", "Claude"),
            "隐藏 Claude"
        );
        assert_eq!(
            super::bar_action_label(Locale::Ja, "refresh", "Codex"),
            "Codex を更新"
        );
        assert_eq!(
            super::bar_action_label(Locale::Ar, "settings", "Claude"),
            "إعدادات Claude…"
        );
        assert_eq!(
            super::bar_action_label(Locale::En, "other", "Codex"),
            "other"
        );
    }
}

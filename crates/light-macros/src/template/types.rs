//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token};

use light_fw_html::Node;

pub(super) struct TemplateArgs {
    pub(super) path: LitStr,
    /// Компонент дочернего модального окна (необязательный).
    pub(super) modal: Option<syn::Path>,
    /// Компонент дополнительного главного окна (необязательный).
    pub(super) window: Option<syn::Path>,
    /// Компоненты окон, открываемых по идентификатору через `open("id")`.
    pub(super) windows: Vec<syn::Path>,
}

impl Parse for TemplateArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;
        let mut modal = None;
        let mut window = None;
        let mut windows = Vec::new();
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if key == "modal" {
                modal = Some(input.parse()?);
            } else if key == "window" {
                window = Some(input.parse()?);
            } else if key == "windows" {
                let content;
                syn::bracketed!(content in input);
                windows = syn::punctuated::Punctuated::<syn::Path, Token![,]>::parse_terminated(
                    &content,
                )?
                .into_iter()
                .collect();
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "неизвестный аргумент #[template]: ожидается `modal = Компонент`, `window = Компонент` или `windows = [A, B]`",
                ));
            }
        }
        Ok(Self {
            path,
            modal,
            window,
            windows,
        })
    }
}

/// Р РµР·СѓР»СЊС‚Р°С‚ Р·Р°РіСЂСѓР·РєРё С€Р°Р±Р»РѕРЅР°: СѓР·Р»С‹ СЃ РЎР‚Р В°Р В·РЎР‚Р ВµРЎв‚¬РЎвЂР Р…Р Р…РЎвЂ№Р СР С‘ include Рё СЃРїРёСЃРѕРє
/// СѓС‡Р°СЃС‚РІСѓСЋС‰РёС… С„Р°Р№Р»РѕРІ (РґР»СЏ РѕС‚СЃР»РµР¶РёРІР°РЅРёСЏ РїРµСЂРµСЃР±РѕСЂРєРё).
pub(super) struct Loaded {
    pub(super) nodes: Vec<Node>,
    /// РџСѓС‚Рё Рє РЎвЂћР В°Р в„–Р В»Р В°Р С РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅРѕ РєРѕСЂРЅСЏ РїР°РєРµС‚Р° (РёР»Рё Р°Р±СЃРѕР»СЋС‚РЅС‹Рµ).
    pub(super) files: Vec<String>,
}

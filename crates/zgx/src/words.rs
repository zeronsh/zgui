//! Words in Chinese, Japanese and Thai, which put no spaces between them:
//! where a double click selects one, ICU4X's dictionaries find it, as
//! Chrome's ICU does (feature `cjk-thai-words`). Elsewhere, and with the
//! feature off, words are Unicode's word boundaries (UAX #29), which take
//! each Chinese, Japanese or Thai character for a word.
//!
//! Only a run of those scripts around the point is segmented, when a word
//! is asked for: nothing runs per frame, and the dictionaries (static data)
//! are paged in when first read.

/// Whether `c` is written without spaces between words and has a
/// dictionary here: Han (Chinese, and Japanese kanji), kana and Thai.
#[cfg_attr(not(feature = "cjk-thai-words"), allow(dead_code))]
fn by_dictionary(c: char) -> bool {
    matches!(c as u32,
        // Ideographic iteration marks (々〆〇).
        0x3005..=0x3007
        // Hiragana, Katakana (with ー) and its phonetic extensions.
        | 0x3040..=0x30FF | 0x31F0..=0x31FF
        // CJK ideographs: Extension A, the unified block, compatibility,
        // and Extensions B on.
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x323AF
        // Half-width katakana.
        | 0xFF66..=0xFF9F
        // Thai.
        | 0x0E01..=0x0E5B)
}

/// The dictionary word around byte `index` of `text`, as its byte range,
/// where the character there is Chinese, Japanese or Thai: `None` elsewhere
/// (and with the feature off), for Unicode's word boundaries to decide.
pub(crate) fn dictionary_word(text: &str, index: usize) -> Option<(usize, usize)> {
    #[cfg(feature = "cjk-thai-words")]
    {
        let index = index.min(text.len());
        let at = text.get(index..)?.chars().next()?;
        if !by_dictionary(at) {
            return None;
        }
        // The run of such text around it: what the dictionaries segment.
        let start = text[..index]
            .char_indices()
            .rev()
            .take_while(|(_, c)| by_dictionary(*c))
            .last()
            .map_or(index, |(i, _)| i);
        let end = text[index..]
            .char_indices()
            .find(|(_, c)| !by_dictionary(*c))
            .map_or(text.len(), |(i, _)| index + i);
        let run = &text[start..end];
        let local = index - start;
        let mut previous = 0;
        for boundary in dictionary::segment(run) {
            if boundary > local {
                return Some((start + previous, start + boundary));
            }
            previous = boundary;
        }
        Some((start + previous, end))
    }
    #[cfg(not(feature = "cjk-thai-words"))]
    {
        let _ = (text, index);
        None
    }
}

#[cfg(feature = "cjk-thai-words")]
mod dictionary {
    use icu_collections::codepointinvlist::CodePointInversionList;
    use icu_provider::prelude::*;
    use icu_segmenter::{
        WordSegmenter,
        options::WordBreakOptions,
        provider::{
            Baked, DictionaryBreakData, SegmenterBreakGraphemeClusterV1,
            SegmenterBreakWordOverrideV1, SegmenterBreakWordV1, SegmenterDictionaryAutoV2,
            SegmenterDictionaryExtendedV2,
        },
    };

    /// ICU4X's Thai dictionary, from its compiled data by
    /// `scripts/extract_thai_dictionary.py` (which keeps it in one table with
    /// Burmese, Khmer and Lao, linking all four): its alphabet's inversion
    /// list (a `u32` count, then the `u32`s), then its trie's store.
    static THAI: &[u8] = include_bytes!("../data/thaidict.bin");

    fn thai() -> Option<DictionaryBreakData<'static>> {
        let word = |at: usize| Some(u32::from_le_bytes(THAI.get(at..at + 4)?.try_into().ok()?));
        let count = word(0)? as usize;
        let alphabet: Vec<u32> = (0..count).map(|i| word(4 + 4 * i)).collect::<Option<_>>()?;
        let trie = THAI.get(4 + 4 * count..)?;
        Some(DictionaryBreakData::ZeroTrie {
            alphabet: CodePointInversionList::try_from_u32_inversion_list_slice(&alphabet).ok()?,
            trie: zerotrie::ZeroTrieSimpleAscii::from_store(zerovec::ZeroVec::new_borrowed(trie)),
        })
    }

    /// The compiled data's rules and Chinese and Japanese dictionary, and
    /// Thai's from this crate; no other South-East Asian dictionary.
    struct Data;
    macro_rules! compiled {
        ($($marker:ty),*) => {$(
            impl DataProvider<$marker> for Data {
                fn load(&self, req: DataRequest) -> Result<DataResponse<$marker>, DataError> {
                    Baked.load(req)
                }
            }
        )*};
    }
    compiled!(
        SegmenterBreakWordV1,
        SegmenterBreakWordOverrideV1,
        SegmenterDictionaryAutoV2,
        SegmenterBreakGraphemeClusterV1
    );
    impl DataProvider<SegmenterDictionaryExtendedV2> for Data {
        fn load(
            &self,
            req: DataRequest,
        ) -> Result<DataResponse<SegmenterDictionaryExtendedV2>, DataError> {
            let missing = || {
                DataErrorKind::IdentifierNotFound.with_req(SegmenterDictionaryExtendedV2::INFO, req)
            };
            if req.id.marker_attributes.as_str() != "thaidict" {
                return Err(missing());
            }
            Ok(DataResponse {
                metadata: Default::default(),
                payload: DataPayload::from_owned(thai().ok_or_else(missing)?),
            })
        }
    }

    thread_local! {
        /// Made when a word is first asked for: Thai's alphabet is read
        /// then; its trie and the rest stay in the binary's static data.
        static SEGMENTER: Option<WordSegmenter> =
            WordSegmenter::try_new_dictionary_unstable(&Data, WordBreakOptions::default()).ok();
    }

    /// The word boundaries in `text`, after its start, as byte offsets.
    pub(super) fn segment(text: &str) -> Vec<usize> {
        SEGMENTER.with(|segmenter| match segmenter {
            Some(segmenter) => segmenter
                .as_borrowed()
                .segment_str(text)
                .filter(|&b| b > 0)
                .collect(),
            None => vec![text.len()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::dictionary_word;

    fn word<'a>(text: &'a str, at: &str) -> Option<&'a str> {
        let index = text.find(at).unwrap();
        dictionary_word(text, index).map(|(start, end)| &text[start..end])
    }

    #[cfg(feature = "cjk-thai-words")]
    #[test]
    fn chinese_japanese_and_thai_words_come_from_the_dictionaries() {
        // 我们在北京大学学习: we / at / Beijing / university / study.
        assert_eq!(word("我们在北京大学学习。", "京"), Some("北京"));
        assert_eq!(word("我们在北京大学学习。", "学"), Some("大学"));
        assert_eq!(word("我们在北京大学学习。", "们"), Some("我们"));
        // 東京に行きます: Tokyo / to / go.
        assert_eq!(word("東京に行きます", "京"), Some("東京"));
        // สวัสดีครับ: hello / (polite).
        assert_eq!(word("สวัสดีครับ", "ดี"), Some("สวัสดี"));
        // Only the run of such text: the Latin beside it is not read.
        assert_eq!(word("hello世界", "界"), Some("世界"));
    }

    #[test]
    fn other_text_is_left_to_unicode_word_boundaries() {
        assert_eq!(word("hello world", "w"), None);
        assert_eq!(
            word("한국어", "국"),
            None,
            "Korean puts spaces between words"
        );
        assert_eq!(dictionary_word("", 0), None);
        assert_eq!(dictionary_word("abc", 3), None);
    }

    #[cfg(not(feature = "cjk-thai-words"))]
    #[test]
    fn without_the_dictionaries_nothing_is_a_dictionary_word() {
        assert_eq!(word("我们在北京大学学习。", "京"), None);
        assert_eq!(word("สวัสดีครับ", "ดี"), None);
    }
}

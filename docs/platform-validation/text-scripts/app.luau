-- Text in every script, as docs/validation.md's table checks it in a real
-- window: `zgx-run --inspect` this directory, select across the marks row
-- (`zgx inspect drag`) and capture it (`zgx inspect snapshot`).
local rows = {
	"Emoji: a👨‍👩‍👧‍👦b 👩🏽‍💻 🇯🇵🇫🇷 🏴󠁧󠁢󠁥󠁮󠁧󠁿 1️⃣ 👍🏽 🏳️‍🌈",
	"Bidi: abc אבג 123 def · שלום 123 עולם · مرحبا 2024 بكم",
	"Marks: café naïve · Tiếng Việt · שָׁלוֹם · नमस्ते · ที่นี่",
	"CJK: これは日本語の文章です。「引用」も、句読点も。这是中文，“引号”！",
}
local children = {}
for i, row in rows do
	table.insert(children, ui.text { id = "row" .. i, text_size = 22, row })
end
table.insert(
	children,
	ui.textarea {
		id = "cjk",
		value = ui.state(
			"cjk",
			"これは日本語の文章です。「引用」も、句読点も正しく折り返されるべきです。ちょっと待ってください！"
		),
		w = 200,
		h = 160,
		wrap = true,
		text_size = 22,
	}
)
return ui.column { gap = 12, p = 12, selectable = true, table.unpack(children) }

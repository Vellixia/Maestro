use core_types::SkillDimension;
use serde_json::json;

use crate::probe::ProbeItem;

/// Build the full calibration probe suite.
/// Each dimension gets 5 probes (LLM-judge probes are always last and
/// skipped if no anchor model is available).
pub fn build_suite() -> Vec<ProbeItem> {
    let mut probes = Vec::new();
    probes.extend(reasoning_probes());
    probes.extend(coding_probes());
    probes.extend(math_probes());
    probes.extend(instruction_following_probes());
    probes.extend(long_context_recall_probes());
    probes.extend(tool_calling_probes());
    probes.extend(structured_output_probes());
    probes.extend(factuality_probes());
    probes.extend(multilingual_probes());
    probes.extend(writing_probes());
    probes
}

/// Probes for a single dimension.
pub fn probes_for(dimension: SkillDimension) -> Vec<ProbeItem> {
    match dimension {
        SkillDimension::Reasoning => reasoning_probes(),
        SkillDimension::Coding => coding_probes(),
        SkillDimension::Math => math_probes(),
        SkillDimension::InstructionFollowing => instruction_following_probes(),
        SkillDimension::LongContextRecall => long_context_recall_probes(),
        SkillDimension::ToolCalling => tool_calling_probes(),
        SkillDimension::StructuredOutput => structured_output_probes(),
        SkillDimension::Factuality => factuality_probes(),
        SkillDimension::Multilingual => multilingual_probes(),
        SkillDimension::Writing => writing_probes(),
    }
}

fn reasoning_probes() -> Vec<ProbeItem> {
    vec![
        ProbeItem::contains(
            SkillDimension::Reasoning,
            "If all apples are fruits and all fruits are food, are all apples food? \
             Answer with just 'yes' or 'no'.",
            &["yes"],
        ),
        ProbeItem::contains(
            SkillDimension::Reasoning,
            "A bat and a ball together cost $1.10. The bat costs $1.00 more than the ball. \
             How much does the ball cost in cents? Answer with just the number.",
            &["5"],
        ),
        ProbeItem::contains(
            SkillDimension::Reasoning,
            "Which is larger: 9.9 or 9.11? Answer with just the larger number.",
            &["9.9"],
        ),
        ProbeItem::contains(
            SkillDimension::Reasoning,
            "If it takes 5 machines 5 minutes to make 5 widgets, how many minutes does it take \
             100 machines to make 100 widgets? Answer with just the number.",
            &["5"],
        ),
        ProbeItem::contains(
            SkillDimension::Reasoning,
            "There are 100 patients. 99 have a disease. A test is 99% accurate. \
             If a patient tests positive, is it more likely they have or do not have the disease? \
             Answer 'have' or 'do not have'.",
            &["have"],
        ),
    ]
}

fn coding_probes() -> Vec<ProbeItem> {
    vec![
        ProbeItem::contains(
            SkillDimension::Coding,
            "What is the output of: print(2 ** 10)? Answer with just the number.",
            &["1024"],
        ),
        ProbeItem::contains(
            SkillDimension::Coding,
            "In Python, what does list.append() return? Answer in one word.",
            &["none"],
        ),
        ProbeItem::contains(
            SkillDimension::Coding,
            "What is the time complexity of binary search? \
             Answer with just the Big-O notation.",
            &["o(log n)", "o(logn)"],
        ),
        ProbeItem::contains(
            SkillDimension::Coding,
            "In Python, what keyword is used to define a generator function? \
             Answer with just the keyword.",
            &["yield"],
        ),
        ProbeItem::contains(
            SkillDimension::Coding,
            "What does SQL SELECT DISTINCT do? Answer in one short sentence.",
            &["duplicate", "unique", "distinct"],
        ),
    ]
}

fn math_probes() -> Vec<ProbeItem> {
    vec![
        ProbeItem::numeric(SkillDimension::Math,
            "What is 17 × 23? Answer with just the number.", 391.0, 0.5),
        ProbeItem::numeric(SkillDimension::Math,
            "What is the square root of 144? Answer with just the number.", 12.0, 0.5),
        ProbeItem::numeric(SkillDimension::Math,
            "Solve for x: 2x + 5 = 13. Answer with just the number.", 4.0, 0.5),
        ProbeItem::numeric(SkillDimension::Math,
            "What is 15% of 240? Answer with just the number.", 36.0, 0.5),
        ProbeItem::numeric(SkillDimension::Math,
            "What is log base 2 of 256? Answer with just the number.", 8.0, 0.5),
    ]
}

fn instruction_following_probes() -> Vec<ProbeItem> {
    vec![
        ProbeItem::exact(
            SkillDimension::InstructionFollowing,
            "Respond with only the word HELLO in all caps. Nothing else.",
            "hello",
        ),
        ProbeItem::contains(
            SkillDimension::InstructionFollowing,
            "Count the words in this sentence and answer with just the number: \
             'The quick brown fox jumps over the lazy dog'",
            &["9"],
        ),
        ProbeItem::contains(
            SkillDimension::InstructionFollowing,
            "Reverse the word 'algorithm'. Answer with just the reversed word.",
            &["mhtirogla"],
        ),
        ProbeItem::contains(
            SkillDimension::InstructionFollowing,
            "List exactly three primary colors separated by commas. No other text.",
            &[","],  // must contain at least one comma → 3-item list
        ),
        ProbeItem::contains(
            SkillDimension::InstructionFollowing,
            "Translate 'hello' to Spanish. Answer with just the Spanish word.",
            &["hola"],
        ),
    ]
}

fn long_context_recall_probes() -> Vec<ProbeItem> {
    // Bury a key fact inside a block of filler text.
    let filler = "The weather in many cities varies considerably across seasons. \
        Scientists have studied these patterns for decades. Various factors \
        influence temperature and precipitation. Urban areas often experience \
        different conditions than rural zones. ";
    let ctx1 = format!(
        "{filler}{filler}{filler}The secret passphrase is DELTA-SEVEN.{filler}{filler}\
         What is the secret passphrase mentioned above? Answer with just the passphrase."
    );
    let ctx2 = format!(
        "{filler}{filler}The inventory count for item XR-42 is 1337 units.{filler}{filler}\
         {filler}What is the inventory count for item XR-42? Answer with just the number."
    );
    vec![
        ProbeItem::contains(SkillDimension::LongContextRecall, &ctx1, &["delta-seven", "delta seven"]),
        ProbeItem::contains(SkillDimension::LongContextRecall, &ctx2, &["1337"]),
        ProbeItem::contains(
            SkillDimension::LongContextRecall,
            &format!(
                "{filler}{filler}IMPORTANT: The color code is VERMILLION-9.{filler}{filler}\
                 What is the color code mentioned in this text? Answer with just the code."
            ),
            &["vermillion-9", "vermillion 9"],
        ),
    ]
}

fn tool_calling_probes() -> Vec<ProbeItem> {
    // These probe whether the model can produce valid structured JSON for a tool call.
    vec![
        ProbeItem::json_schema(
            SkillDimension::ToolCalling,
            "Respond with a JSON object with exactly two keys: \
             'name' (a string) and 'age' (a positive integer). \
             Use name='Alice' and age=30. Return only the JSON.",
            json!({
                "type": "object",
                "required_keys": ["name", "age"]
            }),
        ),
        ProbeItem::json_schema(
            SkillDimension::ToolCalling,
            "Output a JSON object representing a search function call with keys: \
             'function' (value: 'search') and 'query' (any string). Return only the JSON.",
            json!({
                "type": "object",
                "required_keys": ["function", "query"]
            }),
        ),
        ProbeItem::json_schema(
            SkillDimension::ToolCalling,
            "Return a JSON array containing exactly the numbers 1, 2, and 3. \
             Return only the JSON array, nothing else.",
            json!({ "type": "array", "min_items": 3 }),
        ),
    ]
}

fn structured_output_probes() -> Vec<ProbeItem> {
    vec![
        ProbeItem::json_schema(
            SkillDimension::StructuredOutput,
            "Output a JSON object with keys 'title' and 'year'. \
             Use title='The Matrix' and year=1999. Return only valid JSON.",
            json!({
                "type": "object",
                "required_keys": ["title", "year"]
            }),
        ),
        ProbeItem::json_schema(
            SkillDimension::StructuredOutput,
            "Output a JSON array of exactly 3 color names as strings. \
             Return only the JSON array.",
            json!({ "type": "array", "min_items": 3 }),
        ),
        ProbeItem::json_schema(
            SkillDimension::StructuredOutput,
            "Return a JSON object with key 'result' containing the number 42. \
             Return only valid JSON.",
            json!({
                "type": "object",
                "required_keys": ["result"]
            }),
        ),
        ProbeItem::contains(
            SkillDimension::StructuredOutput,
            "Output this data as valid JSON: name is 'Bob', score is 95. \
             Return only the JSON.",
            &["bob", "95"],
        ),
        ProbeItem::contains(
            SkillDimension::StructuredOutput,
            "Is '{\"key\": \"value\"}' valid JSON? Answer with just 'yes' or 'no'.",
            &["yes"],
        ),
    ]
}

fn factuality_probes() -> Vec<ProbeItem> {
    vec![
        ProbeItem::contains(
            SkillDimension::Factuality,
            "What is the capital of France? Answer with just the city name.",
            &["paris"],
        ),
        ProbeItem::contains(
            SkillDimension::Factuality,
            "Who wrote Romeo and Juliet? Answer with just the author's last name.",
            &["shakespeare"],
        ),
        ProbeItem::contains(
            SkillDimension::Factuality,
            "In what year did World War II end? Answer with just the year.",
            &["1945"],
        ),
        ProbeItem::contains(
            SkillDimension::Factuality,
            "What is the chemical formula for water? Answer with just the formula.",
            &["h2o"],
        ),
        ProbeItem::contains(
            SkillDimension::Factuality,
            "How many planets are in our solar system? Answer with just the number.",
            &["8"],
        ),
    ]
}

fn multilingual_probes() -> Vec<ProbeItem> {
    vec![
        ProbeItem::contains(
            SkillDimension::Multilingual,
            "Translate 'good morning' to French. Answer with just the translation.",
            &["bonjour"],
        ),
        ProbeItem::contains(
            SkillDimension::Multilingual,
            "What does 'merci' mean in English? Answer with just the English word.",
            &["thank", "thanks"],
        ),
        ProbeItem::contains(
            SkillDimension::Multilingual,
            "Translate 'house' to Spanish. Answer with just the Spanish word.",
            &["casa"],
        ),
        ProbeItem::contains(
            SkillDimension::Multilingual,
            "What language is 'Guten Morgen' from? Answer with just the language name.",
            &["german"],
        ),
        ProbeItem::contains(
            SkillDimension::Multilingual,
            "What is the Italian word for 'water'? Answer with just the Italian word.",
            &["acqua"],
        ),
    ]
}

fn writing_probes() -> Vec<ProbeItem> {
    // LLM-judge probes — skipped if no anchor model.
    vec![
        ProbeItem::llm_judge(
            SkillDimension::Writing,
            "Write a two-sentence product description for a new type of wireless headphone \
             that focuses on comfort and sound quality.",
            "The response should be exactly two sentences, describe comfort and sound quality, \
             be grammatically correct, and be persuasive. Score 1.0 if all criteria met, \
             0.0 if fewer than 2 sentences or completely off-topic.",
        ),
        ProbeItem::llm_judge(
            SkillDimension::Writing,
            "Write a professional email subject line for a meeting request about Q3 budget review.",
            "The subject line should be professional, specific to Q3 budget, and concise (under 10 words). \
             Score 1.0 if professional and relevant, 0.0 if informal or completely off-topic.",
        ),
        ProbeItem::llm_judge(
            SkillDimension::Writing,
            "Explain what a database index is in exactly one sentence, suitable for a non-technical audience.",
            "The explanation should be one sentence, accurate, and understandable to a non-technical person. \
             Score 1.0 if correct and accessible, 0.5 if correct but technical, 0.0 if wrong or multiple sentences.",
        ),
    ]
}

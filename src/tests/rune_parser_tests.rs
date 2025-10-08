use crate::validation::stake::EdictDetails;
use ordinals_runes::parse_psbt_runes_legacy;

/// Production PSBT data containing real rune transactions
/// Source: src/tests/mocked/psbt.txt
/// Expected edicts: 5 total (validated against src/tests/mocked/edicts.json)
const SAMPLE_STAKE_PSBT: &str = "cHNidP8BAP2MAQIAAAADguH7WONxHVnwsbzm7kRXBUICkhnlcPhcGn3IwTQxQJUCAAAAAP////+C4ftY43EdWfCxvObuRFcFQgKSGeVw+FwafcjBNDFAlQMAAAAA/////4fUVe7rn3YWun8G+cSDtyz2EFobYq9TsvTZRwJQ6+0DBgAAAAD/////BwAAAAAAAAAALGpdKRYBAPDVNq8GjIrlpQkBAACMiuWlCQIAAIislgIFAEdkAwAA7Ne9wyEEIgIAAAAAAAAWABS0N7IbXTh8eY/OjHBrx/lIE9iXCiICAAAAAAAAFgAUtDeyG104fHmPzoxwa8f5SBPYlwoiAgAAAAAAABYAFLQ3shtdOHx5j86McGvH+UgT2JcKIgIAAAAAAAAiUSC1ku5/3o8kP46Jo4Gu8EYLq87+FezlURaWfNLwG9CjAiICAAAAAAAAIlEgtZLuf96PJD+OiaOBrvBGC6vO/hXs5VEWlnzS8BvQowLpHAAAAAAAABYAFIETBw+MIvvvkPQarOiTmTKOt9khAAAAAAABAR8iAgAAAAAAABYAFLQ3shtdOHx5j86McGvH+UgT2JcKAAEBKyICAAAAAAAAIlEgtZLuf96PJD+OiaOBrvBGC6vO/hXs5VEWlnzS8BvQowIBE0DV+vZPtYd13TjjXm5j9ixOpZ0K3oeflNn6PR9Bb9MTysXmPfujc4VAOYniPZoraBLfMTOwAyewgzMh79hOAVaMARcgMnKh1CcKJgYwYhEmedK09jEEf+oaMfVEp+Kq7yG4E2IAAQEfhy0AAAAAAAAWABSBEwcPjCL775D0Gqzok5kyjrfZISICAjHcNrnKLZWoyvX2pJVcMgrdXp7taKOQIZqnQozB4YHsSDBFAiEAwb3z7XgDIAm3Rl0op41l8+9V5z9bkT2kQDMAbofXdrMCIGDuU48YfxQHjmDvC8hG255qKMSawkDDG9nho40qR3UhAQAAAAAAAAAA";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rune_edicts_basic() {
        // Validates core parsing functionality using production PSBT data
        let result = parse_psbt_runes_legacy(SAMPLE_STAKE_PSBT);

        assert!(
            result.is_ok(),
            "Failed to parse PSBT runes: {:?}",
            result.err()
        );

        let rune_data = result.unwrap();
        assert!(!rune_data.edicts.is_empty(), "No edicts found in PSBT");

        // Validates edict count matches reference data from src/tests/mocked/edicts.json
        assert_eq!(
            rune_data.edicts.len(),
            5,
            "Expected 5 edicts, found {}",
            rune_data.edicts.len()
        );
    }

    #[test]
    fn test_rune_edict_details() {
        let result = parse_psbt_runes_legacy(SAMPLE_STAKE_PSBT);
        assert!(result.is_ok());

        let rune_data = result.unwrap();

        // Validates specific edict values against production transaction data
        let first_edict = &rune_data.edicts[0];
        assert_eq!(first_edict.id, "895728:815", "First edict ID mismatch");
        assert_eq!(
            first_edict.amount, 2495169804,
            "First edict amount mismatch"
        );
        assert_eq!(first_edict.output, 1, "First edict output mismatch");

        // Validates secondary edict with different rune type
        let fourth_edict = &rune_data.edicts[3];
        assert_eq!(fourth_edict.id, "895728:886", "Fourth edict ID mismatch");
        assert_eq!(fourth_edict.amount, 100, "Fourth edict amount mismatch");
        assert_eq!(fourth_edict.output, 3, "Fourth edict output mismatch");

        // Validates addresses are properly extracted
        let edicts_with_addresses: Vec<_> = rune_data
            .edicts
            .iter()
            .filter(|edict| edict.address.is_some())
            .collect();

        assert!(
            !edicts_with_addresses.is_empty(),
            "No addresses found in edicts"
        );

        // Validates extracted addresses conform to Bitcoin address standards
        for edict in edicts_with_addresses {
            let address = edict.address.as_ref().unwrap();
            assert!(!address.is_empty(), "Empty address found");
            assert!(
                address.starts_with("bc1")
                    || address.starts_with("1")
                    || address.starts_with("3")
                    || address.starts_with("tb1"),
                "Invalid Bitcoin address format: {}",
                address
            );
        }
    }

    #[test]
    fn test_invalid_psbt_handling() {
        // Validates error handling for malformed base64 input
        let result = parse_psbt_runes_legacy("invalid_base64!");
        assert!(result.is_err(), "Should fail with invalid base64");

        // Validates error handling for empty input
        let result = parse_psbt_runes_legacy("");
        assert!(result.is_err(), "Should fail with empty string");

        // Validates error handling for valid base64 with invalid PSBT structure
        let result = parse_psbt_runes_legacy("dGVzdA==");
        assert!(result.is_err(), "Should fail with invalid PSBT data");
    }

    #[test]
    fn test_parser_integration_readiness() {
        // Validates parser output compatibility with validation system structures
        let result = parse_psbt_runes_legacy(SAMPLE_STAKE_PSBT);
        assert!(result.is_ok());

        let rune_data = result.unwrap();

        // Confirms parsed data maps correctly to EdictDetails structure
        for edict in &rune_data.edicts {
            let edict_detail = EdictDetails {
                id: edict.id.clone(),
                amount: edict.amount,
                address: edict.address.clone(),
                output: edict.output,
            };

            assert!(!edict_detail.id.is_empty(), "Edict ID should not be empty");
            assert!(edict_detail.amount > 0, "Edict amount should be positive");
            assert!(
                edict_detail.id.contains(':'),
                "Rune ID should follow block:tx format"
            );
        }
    }
}

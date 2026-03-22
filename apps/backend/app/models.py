from pydantic import BaseModel, Field


class PrivacyConfig(BaseModel):
    # Entity masking
    mask_names: bool = Field(True, description="Replace detected names with [PERSON_N] tokens")
    mask_emails: bool = Field(True, description="Replace email addresses with [EMAIL_N] tokens")
    mask_phones: bool = Field(True, description="Replace phone numbers with [PHONE_N] tokens")
    mask_ssn: bool = Field(True, description="Replace Social Security Numbers with [SSN_N] tokens")
    mask_addresses: bool = Field(False, description="Replace physical addresses with [ADDRESS_N] tokens")

    # Routing & post-processing
    semantic_routing: bool = Field(False, description="Route sensitive prompts to local inference instead of cloud LLMs")
    rehydrate_responses: bool = Field(True, description="Replace placeholder tokens with real values in LLM responses")

    # Detection tuning
    min_confidence_threshold: float = Field(
        0.75,
        ge=0.0,
        le=1.0,
        description="Minimum NER confidence score required to mask an entity",
    )

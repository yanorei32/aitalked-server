# aitalked server
This is a lightweight web server designed for internal use with [Discord TTS](https://github.com/yanorei32/discord-tts), enabling headless synthesis of speech using VOICEROID and Gynoidtalk engines.

<img width="1114" height="823" alt="image" src="https://github.com/user-attachments/assets/4b42e24e-3873-4951-aa4b-93d2a8f2898f" />


## Features

- 🗣️ **Headless Speech Synthesis**  
  Utilizes the [aitalked](https://github.com/yanorei32/aitalked) crate to interface with `aitalked.dll`, allowing speech generation without GUI interaction.

- 🔊 **Simple API Design**  
  Offers a minimal HTTP API for generating speech. *Note: accent position editing is not supported due to API constraints.*

- 📦 **WAV File Output**  
  Returns synthesized speech as a WAV file in the HTTP response.

- 🧠 **Character Info Extraction**  
  Includes a feature to extract character infos from `info.bin`.

## Use Case

Primarily intended for integration with Discord bots or automation tools requiring Japanese TTS capabilities in a headless environment.

## API Details

### `GET /api/voices`

This endpoint returns a list of available voice profiles that can be used with the TTS engine.

#### Response

The response is a JSON array of voice objects. Each object contains:

- `id` *(string)*: A unique identifier for the voice. This is used as the `voice_id` in the `/api/tts` request.
- `name` *(string)*: The display name of the character or voice.
- `icon` *(string)*: A base64-encoded PNG image representing the character's icon.
- `dialect` *(string)*: Describes the regional dialect used by the voice (e.g., "Standard", "Kansai").
- `gender` *(string)*: Indicates the gender of the voice (e.g., "Male", "Female").
- `background_color` *(string)*: A hex color code representing the character's theme or UI background color.

This endpoint is useful for dynamically populating voice selection UIs or validating available options before making synthesis requests.

### `POST /api/tts`

This endpoint generates speech audio from the provided text and voice parameters. It returns a WAV file upon success or a plain-text error message if the request is invalid.

#### Request

The request must be a JSON object with the following fields:

- `voice_id` *(string)*: The identifier of the voice character to use. This should match one of the IDs returned by the `/api/voices` endpoint.
- `text` *(string)*: The input text to be synthesized into speech.
- `is_kansai` *(boolean)* *(optional)*: If set to `true`, the generated speech will use Kansai dialect.
- `volume` *(number)* *(optional)*: Controls the loudness of the voice. Typically ranges from 0 to 1 (maximum 5).
- `speed` *(number)* *(optional)*: Adjusts the speaking rate. Lower values slow down the speech, higher values speed it up.
- `pitch` *(number)* *(optional)*: Modifies the pitch of the voice. Useful for making the voice sound higher or deeper.
- `range` *(number)* *(optional)*: Controls the pitch variation. A higher range adds more expressiveness.
- `pause_middle` *(number)* *(optional)*: Sets the pause duration after commas or mid-sentence breaks.
- `pause_long` *(number)* *(optional)*: Sets the pause duration after long breaks, such as semicolons.
- `pause_sentence` *(number)* *(optional)*: Sets the pause duration at the end of sentences.

#### Response

- `200 OK`: Returns a WAV file containing the synthesized speech.
- `400 BAD_REQUEST`: Returns a plain-text error message describing the issue (e.g., missing fields, invalid values).

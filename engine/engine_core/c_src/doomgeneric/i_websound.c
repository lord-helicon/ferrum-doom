// Web-facing sound/music module for doomgeneric.

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "deh_str.h"
#include "doomtype.h"
#include "i_sound.h"
#include "m_misc.h"
#include "memio.h"
#include "mus2mid.h"
#include "w_wad.h"
#include "z_zone.h"

#define NUM_CHANNELS 16

typedef struct {
    int id;
    uint8_t *data;
    int len;
} web_song_t;

static boolean use_sfx_prefix = true;
int use_libsamplerate = 0;
float libsamplerate_scale = 1.0f;

extern void DG_RustSfxStart(int channel,
                            const uint8_t *data,
                            int len,
                            int samplerate,
                            int volume,
                            int sep);
extern void DG_RustSfxStop(int channel);
extern void DG_RustSfxUpdateParams(int channel, int volume, int sep);
extern int DG_RustSfxIsPlaying(int channel);

extern void DG_RustMusicRegister(int song_id, const uint8_t *data, int len);
extern void DG_RustMusicUnregister(int song_id);
extern void DG_RustMusicPlay(int song_id, int looping);
extern void DG_RustMusicStop(void);
extern void DG_RustMusicPause(int paused);
extern void DG_RustMusicVolume(int volume);
extern int DG_RustMusicIsPlaying(void);

static void GetSfxLumpName(sfxinfo_t *sfx, char *buf, size_t buf_len)
{
    if (sfx->link != NULL)
    {
        sfx = sfx->link;
    }

    if (use_sfx_prefix)
    {
        M_snprintf(buf, buf_len, "ds%s", DEH_String(sfx->name));
    }
    else
    {
        M_StringCopy(buf, DEH_String(sfx->name), buf_len);
    }
}

static int I_WEB_GetSfxLumpNum(sfxinfo_t *sfx)
{
    char namebuf[9];

    GetSfxLumpName(sfx, namebuf, sizeof(namebuf));

    return W_GetNumForName(namebuf);
}

static void I_WEB_UpdateSound(void)
{
    // Mixer state is maintained on the Rust side.
}

static void I_WEB_UpdateSoundParams(int channel, int vol, int sep)
{
    if (channel < 0 || channel >= NUM_CHANNELS)
    {
        return;
    }

    DG_RustSfxUpdateParams(channel, vol, sep);
}

static int I_WEB_StartSound(sfxinfo_t *sfxinfo, int channel, int vol, int sep)
{
    int lumpnum;
    unsigned int lumplen;
    int samplerate;
    unsigned int length;
    byte *data;

    if (channel < 0 || channel >= NUM_CHANNELS)
    {
        return -1;
    }

    lumpnum = sfxinfo->lumpnum;
    if (lumpnum <= 0)
    {
        lumpnum = I_WEB_GetSfxLumpNum(sfxinfo);
        sfxinfo->lumpnum = lumpnum;
    }

    data = W_CacheLumpNum(lumpnum, PU_STATIC);
    lumplen = W_LumpLength(lumpnum);

    if (lumplen < 8 || data[0] != 0x03 || data[1] != 0x00)
    {
        W_ReleaseLumpNum(lumpnum);
        return -1;
    }

    samplerate = (data[3] << 8) | data[2];
    length = (data[7] << 24) | (data[6] << 16) | (data[5] << 8) | data[4];

    if (length > lumplen - 8 || length <= 48)
    {
        W_ReleaseLumpNum(lumpnum);
        return -1;
    }

    data += 16;
    length -= 32;

    if (length > 0)
    {
        DG_RustSfxStart(channel, data + 8, (int) length, samplerate, vol, sep);
    }

    W_ReleaseLumpNum(lumpnum);

    return channel;
}

static void I_WEB_StopSound(int channel)
{
    if (channel < 0 || channel >= NUM_CHANNELS)
    {
        return;
    }

    DG_RustSfxStop(channel);
}

static boolean I_WEB_SoundIsPlaying(int channel)
{
    if (channel < 0 || channel >= NUM_CHANNELS)
    {
        return false;
    }

    return DG_RustSfxIsPlaying(channel);
}

static void I_WEB_PrecacheSounds(sfxinfo_t *sounds, int num_sounds)
{
    // Deliberately no-op: sounds are decoded on-demand.
    (void) sounds;
    (void) num_sounds;
}

static void I_WEB_ShutdownSound(void)
{
    int i;

    for (i = 0; i < NUM_CHANNELS; ++i)
    {
        DG_RustSfxStop(i);
    }
}

static boolean I_WEB_InitSound(boolean _use_sfx_prefix)
{
    use_sfx_prefix = _use_sfx_prefix;
    return true;
}

static snddevice_t sound_web_devices[] =
{
    SNDDEVICE_SB,
    SNDDEVICE_PAS,
    SNDDEVICE_GUS,
    SNDDEVICE_WAVEBLASTER,
    SNDDEVICE_SOUNDCANVAS,
    SNDDEVICE_AWE32,
};

sound_module_t DG_sound_module =
{
    sound_web_devices,
    arrlen(sound_web_devices),
    I_WEB_InitSound,
    I_WEB_ShutdownSound,
    I_WEB_GetSfxLumpNum,
    I_WEB_UpdateSound,
    I_WEB_UpdateSoundParams,
    I_WEB_StartSound,
    I_WEB_StopSound,
    I_WEB_SoundIsPlaying,
    I_WEB_PrecacheSounds,
};

static boolean I_WEBMUSIC_Init(void)
{
    return true;
}

static void I_WEBMUSIC_Shutdown(void)
{
    DG_RustMusicStop();
}

static void I_WEBMUSIC_SetMusicVolume(int volume)
{
    DG_RustMusicVolume(volume);
}

static void I_WEBMUSIC_PauseMusic(void)
{
    DG_RustMusicPause(1);
}

static void I_WEBMUSIC_ResumeMusic(void)
{
    DG_RustMusicPause(0);
}

static int NextSongId(void)
{
    static int next_song_id = 1;

    if (next_song_id <= 0)
    {
        next_song_id = 1;
    }

    return next_song_id++;
}

static void *I_WEBMUSIC_RegisterSong(void *data, int len)
{
    web_song_t *song;
    uint8_t *midi_bytes = NULL;
    int midi_len = 0;

    if (data == NULL || len <= 0)
    {
        return NULL;
    }

    song = malloc(sizeof(web_song_t));
    if (song == NULL)
    {
        return NULL;
    }

    memset(song, 0, sizeof(*song));
    song->id = NextSongId();

    if (len >= 4
        && ((uint8_t *) data)[0] == 'M'
        && ((uint8_t *) data)[1] == 'U'
        && ((uint8_t *) data)[2] == 'S'
        && ((uint8_t *) data)[3] == 0x1A)
    {
        MEMFILE *src;
        MEMFILE *dst;
        void *buf = NULL;
        size_t buflen = 0;

        src = mem_fopen_read(data, (size_t) len);
        dst = mem_fopen_write();

        if (src == NULL || dst == NULL || mus2mid(src, dst))
        {
            if (src != NULL)
            {
                mem_fclose(src);
            }
            if (dst != NULL)
            {
                mem_fclose(dst);
            }
            free(song);
            return NULL;
        }

        mem_get_buf(dst, &buf, &buflen);

        if (buf == NULL || buflen == 0)
        {
            mem_fclose(src);
            mem_fclose(dst);
            free(song);
            return NULL;
        }

        midi_len = (int) buflen;
        midi_bytes = malloc((size_t) midi_len);
        if (midi_bytes == NULL)
        {
            mem_fclose(src);
            mem_fclose(dst);
            free(song);
            return NULL;
        }

        memcpy(midi_bytes, buf, (size_t) midi_len);

        mem_fclose(src);
        mem_fclose(dst);
    }
    else
    {
        midi_len = len;
        midi_bytes = malloc((size_t) midi_len);
        if (midi_bytes == NULL)
        {
            free(song);
            return NULL;
        }
        memcpy(midi_bytes, data, (size_t) midi_len);
    }

    song->data = midi_bytes;
    song->len = midi_len;

    DG_RustMusicRegister(song->id, song->data, song->len);

    return song;
}

static void I_WEBMUSIC_UnRegisterSong(void *handle)
{
    web_song_t *song = (web_song_t *) handle;

    if (song == NULL)
    {
        return;
    }

    DG_RustMusicUnregister(song->id);

    if (song->data != NULL)
    {
        free(song->data);
    }

    free(song);
}

static void I_WEBMUSIC_PlaySong(void *handle, boolean looping)
{
    web_song_t *song = (web_song_t *) handle;

    if (song == NULL)
    {
        return;
    }

    DG_RustMusicPlay(song->id, looping);
}

static void I_WEBMUSIC_StopSong(void)
{
    DG_RustMusicStop();
}

static boolean I_WEBMUSIC_MusicIsPlaying(void)
{
    return DG_RustMusicIsPlaying() ? true : false;
}

static snddevice_t music_web_devices[] =
{
    SNDDEVICE_GENMIDI,
    SNDDEVICE_GUS,
    SNDDEVICE_SB,
};

music_module_t DG_music_module =
{
    music_web_devices,
    arrlen(music_web_devices),
    I_WEBMUSIC_Init,
    I_WEBMUSIC_Shutdown,
    I_WEBMUSIC_SetMusicVolume,
    I_WEBMUSIC_PauseMusic,
    I_WEBMUSIC_ResumeMusic,
    I_WEBMUSIC_RegisterSong,
    I_WEBMUSIC_UnRegisterSong,
    I_WEBMUSIC_PlaySong,
    I_WEBMUSIC_StopSong,
    I_WEBMUSIC_MusicIsPlaying,
    NULL,
};

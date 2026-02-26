//
// Copyright(C) 1993-1996 Id Software, Inc.
// Copyright(C) 2005-2014 Simon Howard
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// DESCRIPTION:
//	Endianess handling, swapping 16bit and 32bit.
//


#ifndef __I_SWAP__
#define __I_SWAP__

#ifdef FEATURE_SOUND


#ifdef __DJGPP__


#define SHORT(x)  ((signed short) (x))
#define LONG(x)   ((signed int) (x))

#define SYS_LITTLE_ENDIAN


#else  // __DJGPP__


#if defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
#define SHORT(x)  ((signed short) (x))
#define LONG(x)   ((signed int) (x))
#define SYS_LITTLE_ENDIAN
#define doom_wtohs(x) (short int)(x)
#elif defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_BIG_ENDIAN__)
#define SHORT(x)  ((signed short) __builtin_bswap16((unsigned short) (x)))
#define LONG(x)   ((signed int) __builtin_bswap32((unsigned int) (x)))
#define SYS_BIG_ENDIAN
#define doom_wtohs(x) ((short int) __builtin_bswap16((unsigned short) (x)))
#else
#define SHORT(x)  ((signed short) (x))
#define LONG(x)   ((signed int) (x))
#define SYS_LITTLE_ENDIAN
#define doom_wtohs(x) (short int)(x)
#endif


#endif  // __DJGPP__


#else  // FEATURE_SOUND
	
#define SHORT(x)  ((signed short) (x))
#define LONG(x)   ((signed int) (x))

#define SYS_LITTLE_ENDIAN

#endif /* FEATURE_SOUND */

#endif


/* Native acceptance only: reversible default-output changes backed by the actual current speakers. */
#include <CoreAudio/CoreAudio.h>
#include <CoreFoundation/CoreFoundation.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>
int ka_route_begin(uint32_t* original, uint32_t* aggregate) {
    *original=0; *aggregate=0;
    AudioObjectPropertyAddress address={kAudioHardwarePropertyDefaultOutputDevice,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
    UInt32 size=sizeof(AudioDeviceID);
    OSStatus result=AudioObjectGetPropertyData(kAudioObjectSystemObject,&address,0,NULL,&size,original);
    if(result!=noErr||!*original) return (int)result ? (int)result : -1;
    AudioObjectPropertyAddress uid_address={kAudioDevicePropertyDeviceUID,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
    CFStringRef original_uid=NULL; size=sizeof(original_uid);
    result=AudioObjectGetPropertyData(*original,&uid_address,0,NULL,&size,&original_uid);
    if(result!=noErr||!original_uid) return (int)result ? (int)result : -1;
    char unique[128]; snprintf(unique,sizeof(unique),"org.katla.odin.audio.acceptance.%d",(int)getpid());
    CFStringRef uid=CFStringCreateWithCString(NULL,unique,kCFStringEncodingUTF8);
    const void* sub_keys[]={CFSTR(kAudioSubDeviceUIDKey)}; const void* sub_values[]={original_uid};
    CFDictionaryRef sub=CFDictionaryCreate(NULL,sub_keys,sub_values,1,&kCFTypeDictionaryKeyCallBacks,&kCFTypeDictionaryValueCallBacks);
    const void* sub_items[]={sub}; CFArrayRef subs=CFArrayCreate(NULL,sub_items,1,&kCFTypeArrayCallBacks);
    int zero=0; CFNumberRef private_value=CFNumberCreate(NULL,kCFNumberIntType,&zero);
    const void* keys[]={CFSTR(kAudioAggregateDeviceNameKey),CFSTR(kAudioAggregateDeviceUIDKey),CFSTR(kAudioAggregateDeviceSubDeviceListKey),CFSTR(kAudioAggregateDeviceMainSubDeviceKey),CFSTR(kAudioAggregateDeviceIsPrivateKey)};
    const void* values[]={CFSTR("Katla Odin audio acceptance"),uid,subs,original_uid,private_value};
    CFDictionaryRef description=CFDictionaryCreate(NULL,keys,values,5,&kCFTypeDictionaryKeyCallBacks,&kCFTypeDictionaryValueCallBacks);
    result=AudioHardwareCreateAggregateDevice(description,aggregate);
    CFRelease(description);CFRelease(private_value);CFRelease(subs);CFRelease(sub);CFRelease(uid);CFRelease(original_uid);
    if(result!=noErr) return (int)result;
    size=sizeof(AudioDeviceID); result=AudioObjectSetPropertyData(kAudioObjectSystemObject,&address,0,NULL,size,aggregate);
    if(result!=noErr) {AudioHardwareDestroyAggregateDevice(*aggregate);*aggregate=0;return (int)result;}
    return 0;
}
int ka_route_end(uint32_t original,uint32_t aggregate) {
    AudioObjectPropertyAddress address={kAudioHardwarePropertyDefaultOutputDevice,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
    OSStatus result=AudioObjectSetPropertyData(kAudioObjectSystemObject,&address,0,NULL,sizeof(original),&original);
    OSStatus destroy=aggregate?AudioHardwareDestroyAggregateDevice(aggregate):noErr;
    return result!=noErr ? (int)result : (int)destroy;
}

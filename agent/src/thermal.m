#import <Foundation/Foundation.h>
int sm_thermal(void) { @autoreleasepool { return (int)[[NSProcessInfo processInfo] thermalState]; } }

/*

A couple of dices on a tiny 80x160px TFT display

Author: Alfonso de Cala <alfonso@el-magnifico.org>

*/

#include <M5StickCPlus2.h>

#define DOT_SIZE 6
int dot[6][6][2]{
    {{35, 35}},
    {{15, 15}, {55, 55}},
    {{15, 15}, {35, 35}, {55, 55}},
    {{15, 15}, {15, 55}, {55, 15}, {55, 55}},
    {{15, 15}, {15, 55}, {35, 35}, {55, 15}, {55, 55}},
    {{15, 15}, {15, 35}, {15, 55}, {55, 15}, {55, 35}, {55, 55}},
};

float accX = 0;
float accY = 0;
float accZ = 0;

void setup(void) {
    M5.begin();
    M5.Imu.init();

    M5.Lcd.setRotation(1);

    M5.Lcd.fillScreen(TFT_GREEN);

    M5.Lcd.setTextColor(TFT_BLACK);  // Adding a background colour erases
                                     // previous text automatically

    M5.Lcd.setCursor(10, 30);
    M5.Lcd.setTextSize(3);
    M5.Lcd.print("SHAKE ME");
    delay(1000);
}

void draw_dice(int16_t x, int16_t y, int n) {
    M5.Lcd.fillRect(x, y, 70, 70, WHITE);

    for (int d = 0; d < 6; d++) {
        if (dot[n][d][0] > 0) {
            M5.Lcd.fillCircle(x + dot[n][d][0], y + dot[n][d][1], DOT_SIZE,
                              TFT_BLACK);
        }
    }
}

void loop() {
    while (1) {
        M5.Imu.getAccelData(&accX, &accY, &accZ);
        if (accX > 1.5 || accY > 1.5) {
            break;
        }
    }

    M5.Lcd.fillScreen(TFT_GREEN);

    // Draw first dice
    delay(500);  // A little delay to increase suspense :-D
    int number = random(0, 6);
    draw_dice(50, 30, number);

    // Draw second dice
    delay(500);
    number = random(0, 6);
    draw_dice(125, 30, number);
}

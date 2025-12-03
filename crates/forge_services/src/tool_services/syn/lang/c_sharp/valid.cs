using System;
using System.Collections.Generic;

namespace HelloWorld
{
    class Program
    {
        static void Main(string[] args)
        {
            Console.WriteLine("Hello, World!");
            
            var numbers = new List<int> { 1, 2, 3, 4, 5 };
            var sum = 0;
            
            foreach (var number in numbers)
            {
                sum += number;
            }
            
            Console.WriteLine($"Sum: {sum}");
        }
    }
}